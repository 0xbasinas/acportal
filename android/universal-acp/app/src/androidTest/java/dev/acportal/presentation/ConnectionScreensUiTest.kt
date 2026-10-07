package dev.acportal.presentation

import androidx.compose.runtime.*
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.transport.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class ConnectionScreensUiTest {
    @get:Rule val compose=createComposeRule()
    @Test fun detailsExposeActionsAndDisableRepeatedDisconnect() {
        var state:ConnectionState by mutableStateOf(ConnectionState.Connected)
        var reconnects=0;var logs=0
        compose.setContent {PortalTheme {ConnectionDetailsScreen("Workstation","https://host.example",state,{}, {logs++},{reconnects++;state=ConnectionState.Connected},{state=ConnectionState.Disconnected},"Goose")}}
        compose.onNodeWithText("Goose").assertIsDisplayed()
        compose.onNodeWithText("View connection logs").performClick()
        compose.onNodeWithText("Disconnect").performClick()
        compose.onNodeWithText("Disconnect").assertIsNotEnabled()
        compose.onNodeWithText("Reconnect").performClick()
        compose.onNodeWithText("• Connected").assertIsDisplayed()
        compose.runOnIdle {assertEquals(1,logs);assertEquals(1,reconnects)}
    }
    @Test fun pausedLogsFreezeWhileFiltersRemainUsable() {
        var events by mutableStateOf(listOf(connectionDiagnostic(ConnectionState.Connected,0)))
        compose.setContent {PortalTheme {ConnectionLogsScreen(events,{},"Workstation",ConnectionState.Connected)}}
        compose.onNode(isToggleable()).performClick()
        compose.runOnIdle {events=events+connectionDiagnostic(ConnectionState.Failed("private"),1)}
        compose.onNodeWithText("Errors").performClick()
        compose.onNodeWithText("No matching events").assertIsDisplayed()
        compose.onNode(isToggleable()).performClick()
        compose.onNodeWithText("Connection unavailable. Review connection status.").assertIsDisplayed()
        compose.onNodeWithText("private").assertDoesNotExist()
    }
}
