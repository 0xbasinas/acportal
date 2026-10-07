package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.data.LiveSession
import dev.acportal.protocol.*
import dev.acportal.storage.*
import dev.acportal.transport.*
import kotlinx.coroutines.flow.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class PromptHistoryUiTest {
    @get:Rule val compose=createComposeRule()
    private class IdleTransport:AgentTransport {
        override suspend fun connect() {}
        override suspend fun disconnect() {}
        override suspend fun send(message:ByteArray) {error("History must not send a transport message")}
        override fun messages()=emptyFlow<ByteArray>()
        override fun connectionState()=flowOf(ConnectionState.Connected)
    }
    @Test fun selectingHistoryRequiresDraftReplacementAndExplicitSendAndPreservesSelectedContext() {
        val host=HostProfile("h","Host","https://host.example","alias","d",0)
        val info=SessionInfo("s","agent","acp","/workspace")
        val live=LiveSession(host,info,IdleTransport(),SessionState(items=listOf(TimelineItem.Text("prior","user","Earlier prompt\nAttached image: old.png",promptText="Earlier prompt"))))
        live.connection.value=ConnectionState.Connected
        live.attachments.value=listOf(contextReference("file:///workspace/current.kt"))
        var draft="Unsaved draft";var sent:String?=null
        compose.setContent {PortalTheme {androidx.compose.material3.Surface {SessionScreen(live,StoredSession("h","s",WireJson.encodeToString(SessionInfo.serializer(),info),draft="Unsaved draft"),onBack={},onPrompt={text,_->sent=text},onCancel={},onPermission={_,_->},onConfigure={_,_->},onDraft={draft=it},onAddAttachment={_,_->},onRemoveAttachment={})}}}
        fun choose() {
            compose.onNodeWithContentDescription("Add context").performClick()
            compose.onNodeWithText("Prompt history").performClick()
            compose.waitForIdle()
            val instrumentation=androidx.test.platform.app.InstrumentationRegistry.getInstrumentation()
            val screenshot=instrumentation.uiAutomation.takeScreenshot()
            val file=java.io.File(instrumentation.targetContext.getExternalFilesDir("screenshots"),"prompt-history.png")
            file.parentFile!!.mkdirs()
            file.outputStream().use {screenshot.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)}
            screenshot.recycle()
            instrumentation.uiAutomation.executeShellCommand("cp ${file.absolutePath} /sdcard/Download/acportal-prompt-history.png").use {android.os.ParcelFileDescriptor.AutoCloseInputStream(it).readBytes()}
            compose.onNodeWithTag("history:prior").performClick()
        }
        choose()
        compose.onNodeWithText("Replace draft?").assertIsDisplayed()
        compose.onNodeWithText("Cancel").performClick()
        compose.runOnIdle {assertEquals("Unsaved draft",draft);assertNull(sent)}
        choose()
        compose.onNodeWithText("Replace").performClick()
        compose.onNode(hasSetTextAction()).assertTextEquals("Earlier prompt")
        compose.onNodeWithText("current.kt").assertIsDisplayed()
        compose.runOnIdle {assertEquals("Earlier prompt",draft);assertNull(sent);assertEquals(1,live.attachments.value.size)}
        compose.onNodeWithContentDescription("Send message").performClick()
        compose.runOnIdle {assertEquals("Earlier prompt",sent)}
    }
    @Test fun emptyHistoryExplainsItsRetainedConversationScope() {
        compose.setContent {PortalTheme {PromptHistorySheet(emptyList(),{},{error("No prompt available")})}}
        compose.onNodeWithText("No submitted text prompts in this conversation's retained history.").assertIsDisplayed()
        compose.onNodeWithTag("prompt-history").assertDoesNotExist()
    }
}
