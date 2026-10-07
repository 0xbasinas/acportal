package dev.acportal.protocol

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class PermissionReplayTest {
    private fun message(id:String)=buildJsonObject {put("id",id);put("method","session/request_permission");putJsonObject("params") {putJsonObject("toolCall") {put("title",id)};putJsonArray("options") {add(buildJsonObject {put("optionId","allow");put("kind","allow_once")})}}}
    private fun frame(type:String)=buildJsonObject {put("type",type);put("latestSequence",90);put("processing",true);put("gap",true)}
    private fun pending(id:String)=JsonObject(frame("pending_permission")+("message" to message(id)))
    @Test fun snapshotReplacesSavedAndHistoricalPermissionsEvenWhenAcknowledgmentWasEvicted() {
        val old=Permission(JsonPrimitive("saved"),message("saved")["params"].objectValue())
        var state=SessionState(sequence=80,items=listOf(TimelineItem.Text("t","agent","Retained")),permissions=mapOf(old.id.toString() to old))
        state=SessionReducer.reduce(state,frame("replay_start"))
        assertTrue(state.permissions.isEmpty());assertTrue(state.replaying)
        state=SessionReducer.reduce(state,buildJsonObject {put("type","event");put("sequence",81);put("direction","agent");put("message",message("historical"))})
        state=SessionReducer.reduce(state,pending("current"))
        state=SessionReducer.reduce(state,pending("current"))
        assertEquals(listOf("historical"),state.permissions.values.map {it.title})
        assertEquals(1,state.replayPermissions!!.size)
        val restored=WireJson.decodeFromString<SessionState>(WireJson.encodeToString(SessionState.serializer(),state))
        state=SessionReducer.reduce(restored,frame("replay_complete"))
        assertEquals(listOf("current"),state.permissions.values.map {it.title});assertNull(state.replayPermissions)
        assertEquals(90L,state.sequence);assertTrue(state.historyGap);assertEquals("Retained",(state.items.single() as TimelineItem.Text).text)
    }
    @Test fun emptySnapshotRemovesObsoleteRequestsAndLiveRequestsStillWork() {
        var state=SessionReducer.reduce(SessionState(),frame("replay_start"))
        state=SessionReducer.reduce(state,buildJsonObject {put("type","event");put("sequence",1);put("message",message("old"))})
        state=SessionReducer.reduce(state,frame("replay_complete"))
        assertTrue(state.permissions.isEmpty())
        state=SessionReducer.reduce(state,pending("live"))
        assertEquals("live",state.permissions.values.single().title)
    }
}
