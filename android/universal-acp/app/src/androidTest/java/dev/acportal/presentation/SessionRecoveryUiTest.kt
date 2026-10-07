package dev.acportal.presentation

import androidx.compose.runtime.*
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.protocol.*
import dev.acportal.transport.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class SessionRecoveryUiTest {
    @get:Rule val compose=createComposeRule()
    @Test fun stoppedSessionRemainsStoppedAfterDismissingItsDetails() {
        var state by mutableStateOf(SessionState(error="Agent exited",errorOrigin=SessionErrorOrigin.SESSION,sessionClosed=true))
        var sessions=0
        compose.setContent {PortalTheme {SessionRecovery(state,ConnectionState.Disconnected,{state=state.copy(error=null)},{},{sessions++})}}
        compose.onNodeWithText("Session stopped").assertIsDisplayed()
        compose.onNodeWithText("Dismiss").performClick()
        compose.onNodeWithText("Session stopped").assertIsDisplayed()
        compose.onNodeWithText("Reconnect").assertDoesNotExist()
        compose.onNodeWithText("Open Sessions").performClick()
        compose.runOnIdle {assertTrue(state.sessionClosed);assertEquals(1,sessions)}
    }
    @Test fun readFailureOffersRetryAndDismiss() {
        var retries=0;var dismissals=0
        compose.setContent {PortalTheme {ConnectionError("Could not read the connection",{dismissals++},{retries++})}}
        compose.onNodeWithText("Retry").performClick()
        compose.onNodeWithContentDescription("Dismiss error").performClick()
        compose.runOnIdle {assertEquals(1,retries);assertEquals(1,dismissals)}
    }
    @Test fun agentErrorDismissalKeepsConversationAndPendingDecision() {
        var state by mutableStateOf(SessionState(error="Provider declined",processing=true,promptRequestId="pending",items=listOf(TimelineItem.Text("t","user","Keep this draft")),permissions=mapOf("p" to Permission(kotlinx.serialization.json.JsonPrimitive("p"),kotlinx.serialization.json.JsonObject(emptyMap())))))
        var reconnects=0
        compose.setContent {PortalTheme {SessionRecovery(state,ConnectionState.Connected,{state=state.copy(error=null)},{reconnects++},{})}}
        compose.onNodeWithText("Agent request failed").assertIsDisplayed()
        compose.onNodeWithText("Reconnect").assertDoesNotExist()
        compose.onNodeWithText("Dismiss").performClick()
        compose.onNodeWithText("Agent request failed").assertDoesNotExist()
        compose.runOnIdle {assertTrue(state.processing);assertEquals("pending",state.promptRequestId);assertEquals(1,state.items.size);assertEquals(1,state.permissions.size);assertEquals(0,reconnects)}
    }
    @Test fun expiredPairingOffersPairingRatherThanReconnect() {
        var pairs=0;var reconnects=0
        compose.setContent {PortalTheme {SessionRecovery(SessionState(),terminalConnectionFailure(401)!!,{}, {reconnects++},{},{pairs++})}}
        compose.onNodeWithText("Pairing expired").assertIsDisplayed()
        compose.onNodeWithText("Reconnect").assertDoesNotExist()
        compose.onNodeWithText("Pair again").performClick()
        compose.runOnIdle {assertEquals(1,pairs);assertEquals(0,reconnects)}
    }
    @Test fun missingSessionReturnsToSessionList() {
        var sessions=0
        compose.setContent {PortalTheme {SessionRecovery(SessionState(),terminalConnectionFailure(404)!!,{},{},{sessions++})}}
        compose.onNodeWithText("Open Sessions").performClick()
        compose.runOnIdle {assertEquals(1,sessions)}
    }
    @Test fun controllerConflictReconnectRequiresExplicitAction() {
        var reconnects=0
        compose.setContent {PortalTheme {SessionRecovery(SessionState(),terminalConnectionFailure(409)!!,{},{reconnects++},{})}}
        compose.onNodeWithText("Session in use").assertIsDisplayed()
        compose.runOnIdle {assertEquals(0,reconnects)}
        compose.onNodeWithText("Reconnect").performClick()
        compose.runOnIdle {assertEquals(1,reconnects)}
    }
}
