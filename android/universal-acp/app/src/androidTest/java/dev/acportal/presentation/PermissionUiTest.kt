package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.protocol.*
import kotlinx.serialization.json.*
import dev.acportal.data.LiveSession
import dev.acportal.storage.*
import dev.acportal.transport.*
import kotlinx.coroutines.flow.*
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class PermissionUiTest {
    @get:Rule val compose=createComposeRule()
    @Test fun conversationPermissionChoicesWaitUntilReplayCompletes() {
        val transport=object:AgentTransport {
            override suspend fun connect() {}
            override suspend fun disconnect() {}
            override suspend fun send(message:ByteArray) {error("Viewing permissions must not submit")}
            override fun messages()=emptyFlow<ByteArray>()
            override fun connectionState()=flowOf(ConnectionState.Connected)
        }
        val info=SessionInfo("s","mock","acp","/workspace")
        val permission=Permission(JsonPrimitive("p"),WireJson.parseToJsonElement("""{"toolCall":{"title":"Current request"},"options":[{"optionId":"yes","name":"Allow once","kind":"allow_once"},{"optionId":"no","name":"Deny","kind":"reject_once"}]}""").objectValue())
        val live=LiveSession(HostProfile("h","Host","https://host.example","alias","d",0),info,transport,SessionState(replaying=true,permissions=mapOf(permission.id.toString() to permission)))
        live.connection.value=ConnectionState.Connected
        var selected=""
        compose.setContent {PortalTheme("Dark") {androidx.compose.material3.Surface {SessionScreen(live,StoredSession("h","s","{}"),onBack={},onPrompt={_,_->},onCancel={},onPermission={_,option->selected=option},onConfigure={_,_->},onDraft={})}}}
        compose.onNodeWithText("Allow once").assertIsNotEnabled()
        compose.onNodeWithText("Deny").assertIsNotEnabled()
        compose.runOnIdle {live.state.value=live.state.value.copy(replaying=false)}
        compose.onNodeWithText("Allow once").assertIsEnabled().performClick()
        compose.runOnIdle {assertEquals("yes",selected)}
    }
    @Test fun interruptedSessionOffersExplicitResumeOnlyForAdvertisedLoading() {
        val info=SessionInfo("stopped","test-agent","saved-session","/workspace",status="interrupted",initialization=WireJson.parseToJsonElement("""{"agentCapabilities":{"loadSession":true}}""").objectValue())
        var resumed=false
        compose.setContent {PortalTheme("dark") {InterruptedSessionScreen(info,false,{}, {resumed=true},{})}}
        compose.onNodeWithText("Session interrupted").assertIsDisplayed()
        compose.onNodeWithText("Resume session").performClick()
        compose.runOnIdle {org.junit.Assert.assertTrue(resumed)}
    }
    @Test fun interruptedSessionWithoutLoadingOffersANewSession() {
        val info=SessionInfo("stopped","test-agent","saved-session","/workspace",status="interrupted")
        var created=false
        compose.setContent {PortalTheme("dark") {InterruptedSessionScreen(info,false,{}, {},{created=true})}}
        compose.onNodeWithText("Resume session").assertDoesNotExist()
        compose.onNodeWithText("Start new session").performClick()
        compose.runOnIdle {org.junit.Assert.assertTrue(created)}
    }
    @Test fun terminalViewerShowsTruncationAndExitStatus() {
        val snapshot=WireJson.parseToJsonElement("""{"output":"final command output","truncated":true,"exitStatus":{"exitCode":7}}""").objectValue()
        compose.setContent {PortalTheme("dark") {TerminalSnapshot(snapshot)}}
        compose.onNodeWithText("Earlier output omitted").assertIsDisplayed()
        compose.onNodeWithText("final command output").assertIsDisplayed()
        compose.onNodeWithText("Exit code 7").assertIsDisplayed()
    }
    @Test fun signInUsesOnlyProtocolDrivenMethodsAndDisablesRepeatedSelection() {
        val info=SessionInfo("auth","test-agent","","workspace",initialization=WireJson.parseToJsonElement("""{"authMethods":[{"id":"login","name":"Test sign-in","description":"Continue on the host"},{"id":"terminal","name":"Terminal sign-in","type":"terminal"}]}""").objectValue())
        var selected=""
        val waiting=androidx.compose.runtime.mutableStateOf(false)
        compose.setContent {PortalTheme("dark") {AuthenticationPanel(info,true,waiting.value) {selected=it;waiting.value=true}}}
        compose.onNodeWithText("Sign in to test-agent").assertIsDisplayed()
        compose.onNodeWithText("Terminal sign-in").assertDoesNotExist()
        compose.onNodeWithText("Test sign-in").performClick()
        compose.runOnIdle {assertEquals("login",selected)}
        compose.onNodeWithText("Test sign-in").assertIsNotEnabled()
    }
    @Test fun hostWriteShowsSourceAndReviewableDiffBeforeChoosing() {
        val permission=Permission(JsonPrimitive("acpd-write-test"),WireJson.parseToJsonElement("""{"_meta":{"acpdSource":"host-filesystem"},"toolCall":{"title":"Write example.kt","content":[{"type":"diff","path":"example.kt","oldText":"val expired = true","newText":"val expired = false"}]},"options":[{"optionId":"acpd-write-deny","name":"Deny","kind":"reject_once"},{"optionId":"acpd-write-allow","name":"Allow write once","kind":"allow_once"}]}""").objectValue())
        var selected=""
        compose.setContent {PortalTheme("dark") {PermissionRequest(permission) {selected=it}}}
        compose.onNodeWithText("Host permission needed").assertIsDisplayed()
        compose.onNodeWithText("Review proposed change").performClick()
        compose.onNodeWithText("val expired = false",substring=true).assertIsDisplayed()
        compose.onNodeWithText("Open full change").performClick()
        compose.onNodeWithText("Changes").assertIsDisplayed()
        compose.waitUntil(10_000) {compose.onAllNodesWithText("val expired = false").fetchSemanticsNodes().isNotEmpty()}
        compose.runOnIdle {assertEquals("",selected)}
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNodeWithText("Allow write once").performClick()
        compose.runOnIdle {assertEquals("acpd-write-allow",selected)}
    }
    @Test fun onlyAdvertisedPermissionOptionsAreRenderedAndChosen() {
        val permission=Permission(JsonPrimitive(7),WireJson.parseToJsonElement("""{"toolCall":{"title":"Run tests"},"options":[{"optionId":"allowed","name":"Allow once","kind":"allow_once"},{"optionId":"rejected","name":"Deny","kind":"reject_once"}]}""").objectValue())
        var selected=""
        compose.setContent {PortalTheme("dark") {PermissionRequest(permission) {selected=it}}}
        compose.onNodeWithText("Run tests").assertIsDisplayed()
        compose.onNodeWithText("Allow for session").assertDoesNotExist()
        compose.onNodeWithText("Deny").performClick()
        compose.runOnIdle {assertEquals("rejected",selected)}
    }
}
