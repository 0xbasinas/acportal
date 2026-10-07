package dev.acportal.data

import android.net.Uri
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import dev.acportal.MainActivity
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.PortalApplication
import dev.acportal.protocol.*
import dev.acportal.transport.ConnectionState
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.Rule

class ReceivedContentHostTest {
    @get:Rule val compose=createAndroidComposeRule<MainActivity>()
    @Test fun incomingMediaResourcesMetadataAndUsageReachThePhone() {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val repository=(context.applicationContext as PortalApplication).repository
        val maybeHost=runBlocking {repository.hosts.first().firstOrNull()}
        assumeTrue("Requires the paired mock host",maybeHost!=null)
        val host=maybeHost!!
        val stored=runBlocking {repository.create(host!!.id,"mock",repository.api.workspaces(host).first().path)}
        try {
            runBlocking {
            val live=repository.open(host.id,stored.id)
            withTimeout(20_000) {live.connection.first {it is ConnectionState.Connected};live.state.first {!it.replaying}}
            if(live.info.acpSessionId.isBlank()) {
                repository.configure(live,"authenticate",buildJsonObject {put("methodId","mock-login")})
                withTimeout(20_000) {live.metadata.first {it.acpSessionId.isNotBlank()}}
            }
            suspend fun emit(update:JsonObject)=repository.configure(live,"_mock/emit_update",buildJsonObject {put("sessionId",live.info.acpSessionId);put("update",update)})
            val blocks=listOf("image" to AttachmentKind.IMAGE,"audio" to AttachmentKind.AUDIO,"text" to AttachmentKind.FILE,"binary" to AttachmentKind.FILE).map {(name,kind)->loadAttachment(context,Uri.parse("content://dev.acportal.test.attachments/$name"),kind).content}+WireJson.parseToJsonElement("""{"type":"resource_link","uri":"file:///workspace/host-only.md","name":"host-only.md"}""").objectValue()
            blocks.forEach {block->emit(buildJsonObject {put("sessionUpdate","agent_message_chunk");put("content",block)})}
            emit(WireJson.parseToJsonElement("""{"sessionUpdate":"session_info_update","title":"Rich content verified","updatedAt":"2026-10-07T08:00:00Z"}""").objectValue())
            emit(WireJson.parseToJsonElement("""{"sessionUpdate":"usage_update","used":53,"size":200,"cost":{"amount":0.045,"currency":"USD"}}""").objectValue())
            emit(buildJsonObject {put("sessionUpdate","tool_call");put("toolCallId","rich-tool");put("title","Rich tool output");put("status","completed");putJsonArray("content") {blocks.forEach {block->add(buildJsonObject {put("type","content");put("content",block)})}}})
            val state=withTimeout(20_000) {live.state.first {it.items.filterIsInstance<TimelineItem.Content>().size==5 && it.items.filterIsInstance<TimelineItem.Tool>().any {tool->tool.id=="rich-tool"} && it.usage["used"].number()==53L}}
            assertEquals(blocks,state.items.filterIsInstance<TimelineItem.Content>().map {it.content})
            assertEquals(5,state.items.filterIsInstance<TimelineItem.Tool>().single {it.id=="rich-tool"}.content.size)
            assertEquals("Rich content verified",state.sessionInfo["title"].text());assertFalse(state.processing)
            assertEquals(state,WireJson.decodeFromString<SessionState>(WireJson.encodeToString(SessionState.serializer(),state)))
            }
            compose.onNodeWithContentDescription("Sessions tab").performClick()
            compose.waitUntil(20_000) {compose.onAllNodesWithTag("session-${stored.id}").fetchSemanticsNodes().isNotEmpty()}
            compose.onNodeWithTag("session-${stored.id}").performClick()
            compose.waitUntil(20_000) {compose.onAllNodesWithTag("session-timeline").fetchSemanticsNodes().isNotEmpty()}
            compose.onNodeWithText("Rich content verified").assertIsDisplayed()
            compose.onNodeWithTag("session-timeline").performTouchInput {swipeDown()}
            compose.onNodeWithTag("session-timeline").performScrollToIndex(0)
            compose.onNodeWithText("Preview image").assertIsDisplayed()
            capture("received-content")
            compose.onNodeWithText("Preview image").performClick()
            capture("received-image-opening")
            try {compose.waitUntil(10_000) {compose.onAllNodesWithContentDescription("Received image preview").fetchSemanticsNodes().isNotEmpty()}}
            catch(failure:Throwable) {capture("received-preview-failure");throw failure}
            capture("received-image-preview")
            compose.onNodeWithText("Close preview").performClick()
        } finally {runBlocking {repository.delete(stored)}}
        Unit
    }
    private fun capture(name:String) {
        compose.waitForIdle()
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val bitmap=instrumentation.uiAutomation.takeScreenshot()
        val file=java.io.File(compose.activity.getExternalFilesDir("screenshots"),"received.png")
        file.parentFile!!.mkdirs();file.outputStream().use {bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)};bitmap.recycle()
        instrumentation.uiAutomation.executeShellCommand("cp ${file.absolutePath} /sdcard/Download/acportal-$name.png").use {android.os.ParcelFileDescriptor.AutoCloseInputStream(it).readBytes()}
    }
}




