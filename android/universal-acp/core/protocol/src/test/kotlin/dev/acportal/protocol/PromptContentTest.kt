package dev.acportal.protocol

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class PromptContentTest {
    private fun info(vararg capabilities:String)=SessionInfo("id","agent","session","workspace",initialization=buildJsonObject {putJsonObject("agentCapabilities") {putJsonObject("promptCapabilities") {capabilities.forEach {put(it,true)}}}})
    private fun media(type:String)=PromptAttachment("fixture",buildJsonObject {put("type",type);put("data","fixture-payload");put("mimeType","$type/test")})
    @Test fun attachmentTypesRequireTheirAdvertisedCapabilitiesButReferencesDoNot() {
        for((type,capability) in listOf("image" to "image","audio" to "audio","resource" to "embeddedContext")) {
            assertThrows(IllegalArgumentException::class.java) {checkedAttachments(info(),listOf(media(type)))}
            assertEquals(1,checkedAttachments(info(capability),listOf(media(type))).size)
        }
        assertEquals(1,checkedAttachments(info(),listOf(contextReference("file:///workspace/code.kt"))).size)
    }
    @Test fun phoneUrisAndCredentialBearingReferencesAreRejected() {
        listOf("content://phone/document/1","android.resource://phone/1","https://user:password@example.com/file","relative/path").forEach {uri->assertThrows(IllegalArgumentException::class.java) {contextReference(uri)}}
    }
    @Test fun requestRetainsPayloadButTimelineDoesNotCacheAttachmentBytes() {
        val content=promptContent("Review the image",listOf(media("image")))
        assertTrue(content.toString().contains("fixture-payload"))
        val event=buildJsonObject {put("type","event");put("sequence",1);put("direction","client");put("message",request("prompt","session/prompt",buildJsonObject {put("prompt",content)}))}
        val state=SessionReducer.reduce(SessionState(),event)
        val text=(state.items.single() as TimelineItem.Text).text
        assertEquals("Review the image\nAttached image: Image",text)
        assertFalse(WireJson.encodeToString(SessionState.serializer(),state).contains("fixture-payload"))
    }
    @Test fun limitsCountAndEncodedUtf8SizeRatherThanCharacters() {
        assertThrows(IllegalArgumentException::class.java) {checkedAttachments(info(),List(5) {contextReference("file:///workspace/file$it")})}
        val large=PromptAttachment("context",buildJsonObject {put("type","resource");putJsonObject("resource") {put("uri","file:///context");put("text","界".repeat(140_000))}})
        assertThrows(IllegalArgumentException::class.java) {checkedAttachments(info("embeddedContext"),listOf(large))}
    }
}
