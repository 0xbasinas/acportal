package dev.acportal.protocol

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test
import java.util.Base64

class ReceivedContentTest {
    private fun update(sequence:Int,update:JsonObject)=buildJsonObject {put("type","event");put("sequence",sequence);put("direction","agent");putJsonObject("message") {put("method","session/update");putJsonObject("params") {put("update",update)}}}
    @Test fun contentBlocksSurviveReplayAndDoNotMergeTextAcrossMedia() {
        var state=SessionState()
        val blocks=listOf("""{"type":"text","text":"Before"}""","""{"type":"image","mimeType":"image/png","data":"AA=="}""","""{"type":"audio","mimeType":"audio/wav","data":"AA=="}""","""{"type":"resource","resource":{"uri":"file:///a.txt","text":"Resource"}}""","""{"type":"resource_link","uri":"file:///b.txt","name":"b.txt"}""","""{"type":"future","extra":{"safe":"value"}}""","""{"type":"text","text":"After"}""")
        blocks.forEachIndexed {index,block->val event=update(index+1,buildJsonObject {put("sessionUpdate","agent_message_chunk");put("content",WireJson.parseToJsonElement(block))});state=SessionReducer.reduce(state,event);assertEquals(state,SessionReducer.reduce(state,event))}
        assertEquals(7,state.items.size);assertEquals(5,state.items.filterIsInstance<TimelineItem.Content>().size)
        assertEquals("After",(state.items.last() as TimelineItem.Text).text)
        assertEquals(state,WireJson.decodeFromString<SessionState>(WireJson.encodeToString(SessionState.serializer(),state)))
    }
    @Test fun messageIdsSeparateConsecutiveMessages() {
        var state=SessionState()
        listOf("a","a","b").forEachIndexed {index,id->state=SessionReducer.reduce(state,update(index+1,buildJsonObject {put("sessionUpdate","agent_message_chunk");put("messageId",id);putJsonObject("content") {put("type","text");put("text","x")}}))}
        assertEquals(listOf("xx","x"),state.items.filterIsInstance<TimelineItem.Text>().map {it.text})
    }
    @Test fun metadataPatchesAndUsageSnapshotsKeepAuthoritativeValues() {
        var state=SessionReducer.reduce(SessionState(),update(1,WireJson.parseToJsonElement("""{"sessionUpdate":"session_info_update","title":"Named session","updatedAt":"2026-10-07T08:00:00Z"}""").objectValue()))
        state=SessionReducer.reduce(state,update(2,WireJson.parseToJsonElement("""{"sessionUpdate":"session_info_update","title":null}""").objectValue()))
        assertEquals(JsonNull,state.sessionInfo["title"]);assertEquals("2026-10-07T08:00:00Z",state.sessionInfo["updatedAt"].text())
        state=SessionReducer.reduce(state,update(3,WireJson.parseToJsonElement("""{"sessionUpdate":"usage_update","used":5,"size":100,"cost":{"amount":0.045,"currency":"USD"}}""").objectValue()))
        assertEquals(5,state.usage["used"].number());assertEquals("USD",state.usage["cost"].objectValue()["currency"].text())
        state=SessionReducer.reduce(state,update(4,WireJson.parseToJsonElement("""{"sessionUpdate":"usage_update","used":8,"size":100}""").objectValue()))
        assertNull(state.usage["cost"])
    }
    @Test fun mediaDecodingSaveNamesAndLinksRejectUnsafeInputs() {
        val binary=WireJson.parseToJsonElement("""{"type":"resource","resource":{"uri":"file:///tmp/a.bin","blob":"AP8BAg=="}}""").objectValue()
        assertArrayEquals(byteArrayOf(0,-1,1,2),receivedContentBytes(binary));assertEquals("a.bin",receivedContentFilename(binary))
        assertNull(contentWebUrl(WireJson.parseToJsonElement("""{"uri":"content://phone/private"}""").objectValue()))
        assertNull(contentWebUrl(WireJson.parseToJsonElement("""{"uri":"https://user:secret@example.com/"}""").objectValue()))
        assertEquals("https://example.com/a",contentWebUrl(WireJson.parseToJsonElement("""{"uri":"https://example.com/a"}""").objectValue()))
        assertThrows(IllegalArgumentException::class.java) {receivedContentBytes(buildJsonObject {put("type","image");put("data","invalid!base64")})}
        assertThrows(IllegalArgumentException::class.java) {receivedContentBytes(buildJsonObject {put("type","image");put("data",Base64.getEncoder().encodeToString(ByteArray(MAX_RECEIVED_CONTENT_BYTES+1)))})}
        val text=buildJsonObject {put("type","resource");putJsonObject("resource") {put("text","世界")}}
        assertArrayEquals("世界".toByteArray(),receivedContentBytes(text))
    }
    @Test fun transcriptIncludesResourcesWithoutEmbeddedBinaryPayloads() {
        val image=WireJson.parseToJsonElement("""{"type":"image","mimeType":"image/png","data":"private-base64-payload"}""").objectValue()
        val text=WireJson.parseToJsonElement("""{"type":"resource","resource":{"uri":"file:///a.txt","text":"Readable resource"}}""").objectValue()
        val markdown=transcriptMarkdown(SessionInfo("s","a","acp","/workspace"),SessionState(items=listOf(TimelineItem.Content("i","agent",image),TimelineItem.Content("r","agent",text))))
        assertTrue(markdown.contains("Readable resource"));assertTrue(markdown.contains("file:///a.txt"));assertFalse(markdown.contains("private-base64-payload"))
    }
    @Test fun largeRetainedMediaAndStreamedTextHaveVisibleHistoryBounds() {
        var state=SessionState()
        repeat(12) {index->state=SessionReducer.reduce(state,update(index+1,buildJsonObject {put("sessionUpdate","agent_message_chunk");putJsonObject("content") {put("type","image");put("data","A".repeat(700_000));put("mimeType","image/png")}}))}
        assertTrue(state.historyGap);assertTrue(state.items.size in 1..4);assertEquals(12,state.sequence)
        val huge=SessionReducer.reduce(SessionState(),update(1,buildJsonObject {put("sessionUpdate","agent_message_chunk");putJsonObject("content") {put("type","text");put("text","x".repeat(2*1024*1024))}}))
        assertTrue(huge.historyGap);assertEquals(1024*1024,(huge.items.single() as TimelineItem.Text).text.length)
    }
}
