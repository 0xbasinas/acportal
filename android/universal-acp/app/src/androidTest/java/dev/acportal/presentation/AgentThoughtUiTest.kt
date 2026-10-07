package dev.acportal.presentation

import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.data.LiveSession
import dev.acportal.protocol.*
import dev.acportal.storage.*
import dev.acportal.transport.*
import kotlinx.coroutines.flow.*
import org.junit.Rule
import org.junit.Test

class AgentThoughtUiTest {
    @get:Rule val compose=createComposeRule()
    private class IdleTransport:AgentTransport {
        override suspend fun connect() {}
        override suspend fun disconnect() {}
        override suspend fun send(message:ByteArray) {error("Viewing thoughts must not send a request")}
        override fun messages()=emptyFlow<ByteArray>()
        override fun connectionState()=flowOf(ConnectionState.Connected)
    }
    @Test fun conversationShowsExplicitThoughtsAndConnectionStateTakesPrecedence() {
        val info=SessionInfo("s","goose","acp","/workspace")
        val live=LiveSession(HostProfile("h","Host","https://host.example","alias","d",0),info,IdleTransport(),SessionState(items=listOf(TimelineItem.Text("thought","thought","I will inspect the project configuration."),TimelineItem.Text("answer","agent","The project is ready.")),processing=true,replaying=true))
        live.connection.value=ConnectionState.Connected
        compose.setContent {PortalTheme("Dark") {androidx.compose.material3.Surface {SessionScreen(live,StoredSession("h","s",WireJson.encodeToString(SessionInfo.serializer(),info)),onBack={},onPrompt={_,_->error("No prompt")},onCancel={},onPermission={_,_->},onConfigure={_,_->},onDraft={})}}}
        compose.onNodeWithText("Syncing").assertIsDisplayed()
        compose.onNodeWithText("The project is ready.").assertIsDisplayed()
        compose.onNodeWithText("I will inspect the project configuration.").assertDoesNotExist()
        compose.onNodeWithContentDescription("Show agent thoughts").performClick()
        compose.onNodeWithText("I will inspect the project configuration.").assertIsDisplayed()
        compose.runOnIdle {live.state.value=live.state.value.copy(replaying=false)}
        compose.onNodeWithText("Working").assertIsDisplayed()
        compose.runOnIdle {live.connection.value=ConnectionState.Disconnected}
        compose.onNodeWithText("Disconnected").assertIsDisplayed()
        compose.waitForIdle()
        val instrumentation=androidx.test.platform.app.InstrumentationRegistry.getInstrumentation()
        val screenshot=instrumentation.uiAutomation.takeScreenshot()
        val file=java.io.File(instrumentation.targetContext.getExternalFilesDir("screenshots"),"agent-thoughts.png")
        file.parentFile!!.mkdirs();file.outputStream().use {screenshot.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)};screenshot.recycle()
        instrumentation.uiAutomation.executeShellCommand("cp ${file.absolutePath} /sdcard/Download/acportal-agent-thoughts.png").use {android.os.ParcelFileDescriptor.AutoCloseInputStream(it).readBytes()}
    }
    @Test fun thoughtsRequireExplicitExpansionAndRemainCollapsedWhileStreaming() {
        var text by mutableStateOf("Agent-provided progress")
        compose.setContent {PortalTheme("Dark") {AgentThoughtCard("thought") {Text(text)}}}
        compose.onNodeWithText(text).assertDoesNotExist()
        compose.runOnIdle {text="Updated agent-provided progress"}
        compose.onNodeWithText(text).assertDoesNotExist()
        compose.onNodeWithContentDescription("Show agent thoughts").performClick()
        compose.onNodeWithText(text).assertIsDisplayed()
        compose.runOnIdle {text="Next streamed chunk"}
        compose.onNodeWithText(text).assertIsDisplayed()
        compose.onNodeWithContentDescription("Hide agent thoughts").performClick()
        compose.onNodeWithText(text).assertDoesNotExist()
    }
}
