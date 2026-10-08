package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import dev.acportal.protocol.SessionState
import dev.acportal.transport.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class SessionRecoveryLayoutUiTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(45)
    @Test fun compactDarkRecoveryActionsWrapAndRemainExplicit()=check("dark")
    @Test fun compactLightRecoveryActionsWrapAndRemainExplicit()=check("light")
    private fun check(mode:String) {
        var connection:ConnectionState by mutableStateOf(ConnectionState.Failed("Connection needs an explicit retry. ".repeat(8)))
        var state by mutableStateOf(SessionState(error="Last request failed. ".repeat(10)))
        var retries=0;var sessions=0;var pairs=0;var dismissals=0
        compose.setContent {
            val density=LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density,2f)) {
                PortalTheme(mode) {Surface(Modifier.width(320.dp).height(280.dp).testTag("recovery-window")) {
                    SessionRecovery(state,connection,{dismissals++;state=state.copy(error=null)},{retries++},{sessions++},{pairs++})
                }}
            }
        }
        fun action(label:String) {
            val button=compose.onNodeWithText(label).performScrollTo().assertIsDisplayed()
                .assert(SemanticsMatcher.expectValue(SemanticsProperties.Role,Role.Button))
            val bounds=button.getUnclippedBoundsInRoot()
            val window=compose.onNodeWithTag("recovery-window").getUnclippedBoundsInRoot()
            assertTrue("Recovery action exceeds width",bounds.left>=window.left && bounds.right<=window.right)
            assertTrue("Recovery action is under 48 dp",bounds.bottom-bounds.top>=48.dp)
            button.performClick()
        }
        compose.onNodeWithText("Connection lost").assert(SemanticsMatcher.keyIsDefined(SemanticsProperties.Heading))
        action("Dismiss")
        compose.runOnIdle {assertEquals(0,retries);assertEquals(1,dismissals)}
        action("Reconnect")
        compose.runOnIdle {
            assertEquals(1,retries)
            state=state.copy(error="Review the pairing failure. ".repeat(8))
            connection=ConnectionState.Failed("Credentials expired. ".repeat(8),ConnectionFailure.AUTHORIZATION)
        }
        action("Dismiss");action("Pair again")
        compose.runOnIdle {
            assertEquals(1,pairs);assertEquals(0,sessions)
            state=state.copy(error="The agent stopped. ".repeat(8),sessionClosed=true)
            connection=ConnectionState.Disconnected
        }
        action("Dismiss");action("Open Sessions")
        compose.runOnIdle {assertEquals(1,sessions);assertEquals(1,retries);assertEquals(1,pairs);assertEquals(3,dismissals)}
    }
}
