package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.compose.ui.test.*
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.data.HostDetails
import dev.acportal.storage.HostProfile
import dev.acportal.protocol.AgentInfo
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class AgentsListLayoutUiTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(45)
    @Test fun compactDarkSavedListAndEmptyRecoveryRemainReachable()=check("dark")
    @Test fun compactLightSavedListAndEmptyRecoveryRemainReachable()=check("light")
    private fun check(mode:String) {
        val name="Custom development agent ".repeat(8)
        val host=HostProfile("fixture","Development workstation ".repeat(6),"http://127.0.0.1:1","unused","fixture",0)
        var agents by mutableStateOf(listOf(AgentInfo("saved-notice",name,installed=true,status="available")))
        var savedAt:Long? by mutableStateOf(1234L)
        var refreshes=0;var backs=0
        val opened=mutableListOf<String>()
        compose.setContent {
            val density=LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density,2f)) {
                PortalTheme(mode) {Surface(Modifier.width(320.dp).height(280.dp)) {
                    RegistryAgentsScreen(HostDetails(host,agents,emptyList(),emptyList(),emptyList()),{backs++},{refreshes++},{opened+=it},savedAt)
                }}
            }
        }
        val list=compose.onNodeWithTag("agent-list")
        list.performScrollToNode(hasText("Saved agent list",substring=true))
        compose.onNodeWithText("Saved agent list",substring=true).assertIsDisplayed()
        list.performScrollToNode(hasText(name))
        val row=compose.onNodeWithText(name)
        row.assertIsDisplayed().assert(SemanticsMatcher.expectValue(SemanticsProperties.Role,Role.Button)).performClick()
        val layouts=mutableListOf<TextLayoutResult>()
        compose.onNodeWithText(name,useUnmergedTree=true).performSemanticsAction(SemanticsActions.GetTextLayoutResult) {it(layouts)}
        assertTrue("Agent name exceeds two summary lines",layouts.single().lineCount<=2)
        compose.onNodeWithText("Last reported: Available").assertExists()
        compose.runOnIdle {assertEquals(listOf("saved-notice"),opened);assertEquals(0,refreshes);agents=emptyList()}
        list.performScrollToNode(hasText("No agents configured"))
        compose.onNodeWithText("No agents configured").assertIsDisplayed()
        val help="Add an ACP agent to this computer's registry, then refresh."
        list.performScrollToNode(hasText(help));compose.onNodeWithText(help).assertIsDisplayed()
        compose.onNodeWithContentDescription("Refresh agents").assertIsDisplayed().performClick()
        compose.runOnIdle {assertEquals(1,refreshes);assertEquals(1,opened.size);savedAt=null;agents=listOf(AgentInfo("fresh","Fresh agent",installed=true,status="available"))}
        compose.onNodeWithText("Saved agent list",substring=true).assertDoesNotExist()
        list.performScrollToNode(hasText("Fresh agent"));compose.onNodeWithText("Fresh agent").performClick()
        compose.onNodeWithContentDescription("Back").performClick()
        compose.runOnIdle {assertEquals(listOf("saved-notice","fresh"),opened);assertEquals(1,backs);assertEquals(1,refreshes)}
    }
}
