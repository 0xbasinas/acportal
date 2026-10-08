package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.transport.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class ConnectionLogsLayoutUiTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(45)
    @Test fun compactDarkFiltersEventsAndPauseRemainReachable()=check("dark")
    @Test fun compactLightFiltersEventsAndPauseRemainReachable()=check("light")
    @Test fun wideDarkFiltersAndNamedLiveSwitchKeepTheirState()=checkWide("dark")
    @Test fun wideLightFiltersAndNamedLiveSwitchKeepTheirState()=checkWide("light")
    private fun check(mode:String) {
        var events by mutableStateOf(listOf(connectionDiagnostic(ConnectionState.Connected,0),connectionDiagnostic(ConnectionState.Reconnecting(2),1)))
        compose.setContent {
            val density=LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density,2f)) {
                PortalTheme(mode) {Surface(Modifier.width(320.dp).height(280.dp).testTag("logs-fixture")) {ConnectionLogsScreen(events,{},"Development workstation ".repeat(6),ConnectionState.Connected)}}
            }
        }
        val list=compose.onNodeWithTag("connection-log-list")
        fun reveal(text:String) {list.performScrollToNode(hasText(text));compose.onNodeWithText(text).assertIsDisplayed()}
        reveal("Warnings")
        val tab=compose.onNodeWithText("Warnings").getUnclippedBoundsInRoot()
        val window=compose.onNodeWithTag("logs-fixture").getUnclippedBoundsInRoot()
        assertTrue("Filter exceeds compact width",tab.left>=window.left && tab.right<=window.right)
        compose.onNodeWithText("Warnings").performClick()
        compose.onNodeWithText("Warnings").assertIsSelected().assert(SemanticsMatcher.expectValue(SemanticsProperties.Role,Role.Tab))
        val target=compose.onNodeWithText("Warnings").getUnclippedBoundsInRoot()
        assertTrue("Filter target is too small",target.right-target.left>=48.dp && target.bottom-target.top>=48.dp)
        reveal("Reconnecting, attempt 2")
        reveal("Live updates")
        list.performScrollToNode(isToggleable());compose.onNodeWithContentDescription("Live updates").assertIsDisplayed().assertIsOn().performClick().assertIsOff()
        compose.runOnIdle {events=events+connectionDiagnostic(ConnectionState.Failed("private fixture detail"),2)}
        reveal("Errors");compose.onNodeWithText("Errors").performClick()
        reveal("No matching events")
        compose.onNodeWithText("Copy").assertIsNotEnabled()
        list.performScrollToNode(isToggleable());compose.onNode(isToggleable()).performClick()
        reveal("Connection unavailable. Review connection status.")
        compose.onNodeWithText("private fixture detail").assertDoesNotExist()
        compose.onNodeWithText("Copy").assertIsEnabled()
        reveal("All");compose.onNodeWithText("All").performClick()
        reveal("WebSocket connected")
        reveal("Connection events only · latest 200")
    }
    private fun checkWide(mode:String) {
        var events by mutableStateOf(listOf(connectionDiagnostic(ConnectionState.Connected,0)))
        var backs=0
        compose.setContent {
            PortalTheme(mode) {Surface(Modifier.width(800.dp).height(700.dp).testTag("logs-fixture")) {
                ConnectionLogsScreen(events,{backs++},"Wide workstation",ConnectionState.Connected)
            }}
        }
        val window=compose.onNodeWithTag("logs-fixture").getUnclippedBoundsInRoot()
        assertEquals("Fixture requires an actual wide device window",800.dp,window.right-window.left)
        val live=compose.onNodeWithContentDescription("Live updates")
        live.assertIsDisplayed().assertIsOn().assert(SemanticsMatcher.expectValue(SemanticsProperties.Role,Role.Switch))
        val toggleBounds=live.getUnclippedBoundsInRoot()
        assertTrue("Switch target is too small",toggleBounds.right-toggleBounds.left>=48.dp && toggleBounds.bottom-toggleBounds.top>=48.dp)
        live.performClick().assertIsOff()
        compose.runOnIdle {events=events+connectionDiagnostic(ConnectionState.Failed("private wide detail"),1)}
        compose.onNodeWithText("Errors").performClick().assertIsSelected()
        compose.onNodeWithText("All").assertIsNotSelected()
        compose.onNodeWithText("No matching events").assertIsDisplayed()
        compose.onNodeWithText("Copy").assertIsNotEnabled()
        live.performClick().assertIsOn()
        compose.onNodeWithText("Connection unavailable. Review connection status.").assertIsDisplayed()
        compose.onNodeWithText("private wide detail").assertDoesNotExist()
        compose.onNodeWithText("Copy").assertIsEnabled()
        compose.onNodeWithText("All").performClick().assertIsSelected()
        compose.onNodeWithText("WebSocket connected").assertIsDisplayed()
        compose.onNodeWithContentDescription("Back").performClick()
        compose.runOnIdle {assertEquals(1,backs)}
    }
}
