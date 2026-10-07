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
import dev.acportal.transport.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class ConnectionLogsLayoutUiTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(45)
    @Test fun compactDarkFiltersEventsAndPauseRemainReachable()=check("dark")
    @Test fun compactLightFiltersEventsAndPauseRemainReachable()=check("light")
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
        reveal("Reconnecting, attempt 2")
        reveal("Live updates")
        list.performScrollToNode(isToggleable());compose.onNode(isToggleable()).assertIsDisplayed().performClick()
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
}
