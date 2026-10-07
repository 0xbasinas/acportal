package dev.acportal.presentation

import android.graphics.Bitmap
import android.util.Base64
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.protocol.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder

class ReceivedContentUiTest {
    @get:Rule val compose=createComposeRule()
    @Test fun imagePreviewRequiresATapAndUsesEmbeddedData() {
        val bytes=ByteArrayOutputStream().use {out->val bitmap=Bitmap.createBitmap(32,24,Bitmap.Config.ARGB_8888);bitmap.eraseColor(android.graphics.Color.GREEN);bitmap.compress(Bitmap.CompressFormat.PNG,100,out);bitmap.recycle();out.toByteArray()}
        val block=buildJsonObject {put("type","image");put("mimeType","image/png");put("data",Base64.encodeToString(bytes,Base64.NO_WRAP));put("uri","https://example.invalid/no-fetch.png")}
        compose.setContent {PortalTheme {ReceivedContentView(block)}}
        compose.onNodeWithContentDescription("Received image preview").assertDoesNotExist()
        compose.onNodeWithText("Preview image").performClick()
        compose.waitUntil(10_000) {compose.onAllNodesWithContentDescription("Received image preview").fetchSemanticsNodes().isNotEmpty()}
        compose.onNodeWithContentDescription("Received image preview").assertIsDisplayed()
        compose.onNodeWithText("Close preview").performClick()
        compose.onNodeWithContentDescription("Received image preview").assertDoesNotExist()
    }
    @Test fun embeddedTextAndResourceReferencesRemainInspectable() {
        val block=WireJson.parseToJsonElement("""{"type":"resource","resource":{"uri":"file:///workspace/readme.md","mimeType":"text/plain","text":"Embedded resource text"}}""").objectValue()
        compose.setContent {PortalTheme {ReceivedContentView(block)}}
        compose.onNodeWithText("Embedded resource text").assertDoesNotExist()
        compose.onNodeWithText("Show resource text").performClick()
        compose.onNodeWithText("Embedded resource text").assertIsDisplayed()
        compose.onNodeWithText("Copy URI").assertIsDisplayed()
        compose.onNodeWithText("Open link").assertDoesNotExist()
        compose.onNodeWithText("Save content").assertIsEnabled()
    }
    @Test fun webReferencesRequireConfirmationAndFutureTypesStayVisible() {
        val block=WireJson.parseToJsonElement("""{"type":"resource_link","uri":"https://example.invalid/resource","name":"Reference","description":"Host resource"}""").objectValue()
        compose.setContent {PortalTheme {ReceivedContentView(block)}}
        compose.onNodeWithText("Open resource link?").assertDoesNotExist()
        compose.onNodeWithText("Open link").performClick()
        compose.onNodeWithText("Open resource link?").assertIsDisplayed()
        compose.onNodeWithText("Cancel").performClick()
        compose.onNodeWithText("Open resource link?").assertDoesNotExist()
    }
    @Test fun unsupportedContentIsVisibleWithoutOfferingAnInvalidSave() {
        compose.setContent {PortalTheme {ReceivedContentView(buildJsonObject {put("type","future");put("name","Future result")})}}
        compose.onNodeWithText("Future result").assertIsDisplayed()
        compose.onNodeWithText("This content type is not supported yet.").assertIsDisplayed()
        compose.onNodeWithText("Save content").assertDoesNotExist()
    }
    @Test fun audioDoesNotAutoplayAndCanBeStopped() {
        val size=32_000*3
        val bytes=ByteBuffer.allocate(44+size).order(ByteOrder.LITTLE_ENDIAN).put("RIFF".toByteArray()).putInt(36+size).put("WAVEfmt ".toByteArray()).putInt(16).putShort(1).putShort(1).putInt(16000).putInt(32000).putShort(2).putShort(16).put("data".toByteArray()).putInt(size).put(ByteArray(size)).array()
        val block=buildJsonObject {put("type","audio");put("mimeType","audio/wav");put("data",Base64.encodeToString(bytes,Base64.NO_WRAP))}
        compose.setContent {PortalTheme {ReceivedContentView(block)}}
        compose.onNodeWithText("Stop audio").assertDoesNotExist()
        compose.onNodeWithText("Play audio").performClick()
        compose.waitUntil(10_000) {compose.onAllNodesWithText("Stop audio").fetchSemanticsNodes().isNotEmpty()}
        compose.onNodeWithText("Stop audio").performClick()
        compose.onNodeWithText("Play audio").assertIsDisplayed()
    }
    @Test fun agentDetailsShowOnlyReportedUsage() {
        val usage=WireJson.parseToJsonElement("""{"used":53,"size":200,"cost":{"amount":0.045,"currency":"USD"}}""").objectValue()
        compose.setContent {PortalTheme {AgentDetailsScreen(SessionInfo("s","mock","acp","/workspace"),{},usage=usage,sessionInfo=buildJsonObject {put("title","Reported title")})}}
        compose.onNodeWithText("53 / 200").assertIsDisplayed()
        compose.onNodeWithText("0.045 USD").assertIsDisplayed()
        compose.onNodeWithText("Reported title").assertIsDisplayed()
    }
}
