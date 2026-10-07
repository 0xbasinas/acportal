package dev.acportal.presentation

import androidx.compose.runtime.*
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.data.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class McpServersUiTest {
    @get:Rule val compose=createComposeRule()
    @Test fun addEditDisableAndRemoveUseRealDefinitions() {
        var servers by mutableStateOf(emptyList<McpDefinition>())
        compose.setContent {PortalTheme {McpServersScreen("Workstation",servers,false,{}, {updated,done->servers=updated;done()})}}
        compose.onNodeWithText("+  Add server").performClick()
        compose.onNodeWithText("Server name").performTextInput("Project docs")
        compose.onNodeWithText("HTTP",substring=true).performClick()
        compose.onNodeWithText("Server URL").performTextInput("https://docs.example.com/mcp")
        save()
        compose.onNodeWithText("Project docs").assertIsDisplayed()
        compose.runOnIdle {assertEquals("http",servers.single().transport)}
        compose.onNodeWithText("Project docs").performClick()
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Use for new sessions"))
        compose.onNode(isToggleable()).performClick()
        save()
        compose.onNodeWithText("Disabled").assertIsDisplayed()
        compose.runOnIdle {assertEquals(0,mcpWire(servers).size)}
        compose.onNodeWithText("Project docs").performClick()
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Remove server"))
        compose.onNodeWithText("Remove server").performClick()
        compose.onNodeWithText("Project docs").assertDoesNotExist()
        compose.runOnIdle {assertTrue(servers.isEmpty())}
    }
    @Test fun invalidExecutableDoesNotSave() {
        compose.setContent {PortalTheme {McpServersScreen("Workstation",emptyList(),false,{}, {_,_->fail("Invalid definition saved")})}}
        compose.onNodeWithText("+  Add server").performClick()
        compose.onNodeWithText("Server name").performTextInput("Workspace tools")
        compose.onNodeWithText("Executable on host").performTextInput("relative-tool")
        save()
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Enter an absolute executable path on this host"))
        compose.onNodeWithText("Enter an absolute executable path on this host").assertIsDisplayed()
    }
    private fun save() {
        androidx.test.espresso.Espresso.closeSoftKeyboard()
        compose.waitForIdle()
        if(compose.onAllNodes(hasText("Save server") and hasAnyAncestor(hasScrollAction())).fetchSemanticsNodes().isNotEmpty())compose.onNode(hasScrollAction() and !hasSetTextAction()).performScrollToNode(hasText("Save server"))
        compose.onNodeWithText("Save server").performClick()
    }
}
