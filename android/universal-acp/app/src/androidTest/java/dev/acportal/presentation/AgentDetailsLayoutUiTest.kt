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
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.protocol.AgentInfo
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class AgentDetailsLayoutUiTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(45)
    @Test fun compactDarkSavedDiscoveryRequiresExplicitRefresh()=check("dark")
    @Test fun compactLightSavedDiscoveryRequiresExplicitRefresh()=check("light")
    private fun check(mode:String) {
        val executable="C:/tools/"+"long-directory/".repeat(12)+"agent.exe"
        val reason="The configured working directory is unavailable. ".repeat(8)
        var savedAt:Long? by mutableStateOf(1234L)
        var loading by mutableStateOf(false)
        var agent by mutableStateOf(AgentInfo("custom","Long custom agent name ".repeat(8),installed=true,status="misconfigured",executable=executable,reason=reason))
        var refreshes=0;var launches=0
        compose.setContent {
            val density=LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density,2f)) {
                PortalTheme(mode) {Surface(Modifier.width(320.dp).height(280.dp).testTag("agent-fixture")) {
                    RegistryAgentDetailsScreen(agent,"Development workstation ".repeat(6),loading,{}, {refreshes++;loading=true},{launches++},savedAt)
                }}
            }
        }
        compose.onNodeWithText("Saved agent list",substring=true).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(executable).performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(reason).performScrollTo().assertIsDisplayed()
        val action=compose.onNodeWithText("New session")
        action.performScrollTo().assertIsDisplayed().assertIsNotEnabled()
        val bounds=action.getUnclippedBoundsInRoot()
        val window=compose.onNodeWithTag("agent-fixture").getUnclippedBoundsInRoot()
        assertTrue("Launch action is clipped",bounds.top>=window.top && bounds.bottom<=window.bottom)
        compose.onNodeWithContentDescription("Refresh agent").performClick().assertIsNotEnabled()
        compose.runOnIdle {assertEquals(1,refreshes);assertEquals(0,launches);agent=agent.copy(status="available",reason=null)}
        action.performScrollTo().assertIsNotEnabled()
        compose.runOnIdle {loading=false}
        action.assertIsNotEnabled()
        compose.runOnIdle {savedAt=null}
        action.performScrollTo().assertIsEnabled().performClick()
        compose.runOnIdle {assertEquals(1,launches);assertEquals(1,refreshes)}
    }
}
