package dev.acportal.protocol

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class PromptHistoryTest {
    @Test fun clientPromptHistoryRetainsOnlyItsTextAndNeverEmbeddedPayloads() {
        val envelope=WireJson.parseToJsonElement("""{"type":"event","sequence":1,"direction":"client","message":{"jsonrpc":"2.0","id":"p","method":"session/prompt","params":{"prompt":[{"type":"text","text":"Keep exact\nspacing"},{"type":"resource","resource":{"uri":"attachment://file","text":"private file contents"}},{"type":"image","data":"private image payload","mimeType":"image/png"}]}}}""").objectValue()
        val state=SessionReducer.reduce(SessionState(),envelope)
        assertEquals("Keep exact\nspacing",promptHistory(state).single().text)
        assertFalse(WireJson.encodeToString(SessionState.serializer(),state).contains("private file contents"))
        assertFalse(WireJson.encodeToString(SessionState.serializer(),state).contains("private image payload"))
        assertEquals(state,WireJson.decodeFromString(SessionState.serializer(),WireJson.encodeToString(SessionState.serializer(),state)))
    }
    @Test fun historyUsesNewestDistinctSubmittedPromptsAndHonorsRetainedState() {
        val items=(0..24).map {TimelineItem.Text("$it","user","Prompt $it",promptText="Prompt $it")}+TimelineItem.Text("repeat","user","Prompt 24",promptText="Prompt 24")
        val history=promptHistory(SessionState(items=items))
        assertEquals(20,history.size);assertEquals("repeat",history.first().id);assertEquals("Prompt 5",history.last().text)
        assertTrue(promptHistory(SessionState(localHistoryCleared=true)).isEmpty())
    }
    @Test fun legacyLabelsAgentEchoesAndAttachmentOnlyPromptsAreNotReusableText() {
        val state=SessionState(items=listOf(TimelineItem.Text("old","user","Attached image: Image"),TimelineItem.Text("agent","agent","Agent response",promptText="Do not use"),TimelineItem.Text("file","user","Attached file: notes",promptText="")))
        assertTrue(promptHistory(state).isEmpty())
        assertEquals("one\ntwo",promptText(WireJson.parseToJsonElement("""[{"type":"text","text":"one"},{"type":"resource_link","uri":"file:///secret"},{"type":"text","text":"two"}]""").arrayValue()))
    }
}
