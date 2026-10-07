package dev.acportal.presentation

import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.data.HostDetails
import dev.acportal.protocol.*
import dev.acportal.storage.HostProfile
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class WorkspaceBrowserUiTest {
    @get:Rule val compose=createComposeRule()
    @Test fun loadingAndFailureHideOldResultsAndPreventSelection() {
        val loading=mutableStateOf(true)
        val failure=mutableStateOf<String?>(null)
        var selected=0;var retries=0
        compose.setContent {PortalTheme {WorkspacePicker(listOf(Workspace("/old","Old result")),"/new",{selected++},{},loading.value,failure.value,{retries++})}}
        compose.onNodeWithText("Loading folders").assertIsDisplayed()
        compose.onNodeWithText("Old result").assertDoesNotExist()
        compose.onNodeWithText("Use this folder").assertIsNotEnabled()
        compose.runOnIdle {loading.value=false;failure.value="Host could not be reached."}
        compose.onNodeWithText("Could not open this folder").assertIsDisplayed()
        compose.onNodeWithText("Old result").assertDoesNotExist()
        compose.onNodeWithText("No subfolders.",substring=true).assertDoesNotExist()
        compose.onNodeWithText("Use this folder").assertIsNotEnabled()
        compose.onNodeWithText("Retry").performClick()
        compose.runOnIdle {assertEquals(1,retries);assertEquals(0,selected)}
    }
    @Test fun dialogRetryKeepsThePathAndSelectionRequiresAnExplicitStart() {
        val host=HostProfile("h","Host","https://host.example","alias","d",0)
        val details=HostDetails(host,listOf(AgentInfo("mock","Mock ACP",installed=true,status="available")),listOf(Workspace("/projects","Projects")),emptyList(),emptyList())
        val loading=mutableStateOf(false);val failure=mutableStateOf<String?>(null)
        val folders=mutableStateOf<List<Workspace>?>(null)
        val requests=mutableListOf<String>();val launches=mutableListOf<String>()
        compose.setContent {PortalTheme {NewSessionScreen(details,folders.value,loading.value,{},{_,path->requests+=path;loading.value=true;failure.value=null},{_,path->launches+=path},browseError=failure.value)}}
        compose.onNodeWithText("Browse folders").performClick()
        compose.onNodeWithContentDescription("Browse Projects").performClick()
        compose.onNodeWithText("Loading folders").assertIsDisplayed()
        compose.onNodeWithText("Use this folder").assertIsNotEnabled()
        compose.runOnIdle {loading.value=false;failure.value="Host could not be reached."}
        compose.onNodeWithText("Could not open this folder").assertIsDisplayed()
        compose.onNodeWithText("Retry").performClick()
        compose.runOnIdle {assertEquals(listOf("/projects","/projects"),requests);assertTrue(launches.isEmpty());loading.value=false;folders.value=emptyList()}
        compose.onNodeWithText("No subfolders. You can use the current folder.").assertIsDisplayed()
        compose.onNodeWithText("Use this folder").performClick()
        compose.onNodeWithText("Choose workspace").assertDoesNotExist()
        compose.runOnIdle {assertTrue(launches.isEmpty())}
        compose.onNodeWithText("Start session").performClick()
        compose.runOnIdle {assertEquals(listOf("/projects"),launches)}
    }
    @Test fun missingRootsDoNotSuggestUsingANonexistentCurrentFolder() {
        compose.setContent {PortalTheme {WorkspacePicker(emptyList(),null,{},{})}}
        compose.onNodeWithText("No allowed folders are available. Configure workspace roots on the host.").assertIsDisplayed()
        compose.onNodeWithText("Use this folder").assertDoesNotExist()
    }
    @Test fun accessBrowserFailureCanCloseWithoutOpeningAPolicy() {
        val host=HostProfile("h","Host","https://host.example","alias","d",0)
        val details=HostDetails(host,emptyList(),listOf(Workspace("/projects","Projects")),emptyList(),emptyList())
        val failure=mutableStateOf<String?>(null)
        var opens=0;var requests=0
        compose.setContent {PortalTheme {WorkspaceAccessListScreen(details,listOf(Workspace("/old","Old result")),false,{}, {requests++;failure.value="Host could not be reached."},{opens++},failure.value)}}
        compose.onNodeWithText("Browse folders").performClick()
        compose.onNodeWithContentDescription("Browse Projects").performClick()
        compose.onNodeWithText("Could not open this folder").assertIsDisplayed()
        compose.onNodeWithText("Use this folder").assertIsNotEnabled()
        compose.onNodeWithText("Old result").assertDoesNotExist()
        compose.onNodeWithText("Close").performClick()
        compose.onNodeWithText("Choose workspace").assertDoesNotExist()
        compose.runOnIdle {assertEquals(1,requests);assertEquals(0,opens)}
    }
}
