package dev.acportal.protocol

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class AgentThoughtTest {
    private fun event(sequence:Int,kind:String,text:String,id:String="thought")=buildJsonObject {
        put("type","event");put("sequence",sequence);put("direction","agent")
        putJsonObject("message") {put("method","session/update");putJsonObject("params") {putJsonObject("update") {
            put("sessionUpdate",kind);put("messageId",id);putJsonObject("content") {put("type","text");put("text",text)}
        }}}
    }
    @Test fun streamedThoughtsStaySeparateFromAnswersAndReplayIsIdempotent() {
        val first=event(1,"agent_thought_chunk","Inspect ")
        var state=SessionReducer.reduce(SessionState(),first)
        assertEquals(state,SessionReducer.reduce(state,first))
        state=SessionReducer.reduce(state,event(2,"agent_thought_chunk","the config"))
        state=SessionReducer.reduce(state,event(3,"agent_message_chunk","Done","answer"))
        state=SessionReducer.reduce(state,event(4,"agent_thought_chunk","Next step","next"))
        assertEquals(listOf("thought","agent","thought"),state.items.filterIsInstance<TimelineItem.Text>().map {it.role})
        assertEquals("Inspect the config",(state.items.first() as TimelineItem.Text).text)
        val restored=WireJson.decodeFromString<SessionState>(WireJson.encodeToString(SessionState.serializer(),state))
        assertEquals(state,restored);assertTrue(promptHistory(state).isEmpty())
        val transcript=transcriptMarkdown(SessionInfo("s","goose","acp","/workspace"),state)
        assertTrue(transcript.contains("## Agent thoughts · goose\n\nInspect the config"))
        assertTrue(transcript.contains("## goose\n\nDone"))
    }
    @Test fun thoughtsUseTheSameBoundedRetentionAsMessages() {
        val state=SessionReducer.reduce(SessionState(),event(1,"agent_thought_chunk","x".repeat(2*1024*1024)))
        assertTrue(state.historyGap);assertEquals(1024*1024,(state.items.single() as TimelineItem.Text).text.length)
    }
    @Test fun nonTextThoughtContentKeepsItsRoleAndPayload() {
        val incoming=event(1,"agent_thought_chunk","")
        val message=incoming["message"].objectValue();val params=message["params"].objectValue();val update=params["update"].objectValue()
        val content=buildJsonObject {put("type","resource_link");put("uri","file:///workspace/config");put("name","Config")}
        val state=SessionReducer.reduce(SessionState(),JsonObject(incoming+("message" to JsonObject(message+("params" to JsonObject(params+("update" to JsonObject(update+("content" to content)))))))))
        val item=state.items.single() as TimelineItem.Content
        assertEquals("thought",item.role);assertEquals(content,item.content)
    }
}
