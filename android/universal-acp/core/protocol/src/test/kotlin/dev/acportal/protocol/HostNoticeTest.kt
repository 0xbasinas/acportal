package dev.acportal.protocol

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class HostNoticeTest {
    private fun event(sequence:Long,message:String,direction:String="agent")=WireJson.parseToJsonElement("""{"type":"event","sequence":$sequence,"direction":"$direction","message":$message}""").objectValue()
    private fun toolUpdate(sequence:Long,kind:String,status:String,meta:String,direction:String="host")=event(sequence,"""{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"$kind","toolCallId":"w1","title":"Write a.txt","kind":"edit","status":"$status","_meta":$meta,"content":[{"type":"content","content":{"type":"text","text":"notice"}}]}}}""",direction)

    @Test fun hostWriteOutcomesAreKeptOnTheToolAndOnlyTrustedFromTheHost() {
        val pending=SessionReducer.reduce(SessionState(),toolUpdate(1,"tool_call","pending","""{"acpdSource":"host-filesystem"}"""))
        assertNull((pending.items.single() as TimelineItem.Tool).hostWrite)
        val changed=SessionReducer.reduce(pending,toolUpdate(2,"tool_call_update","failed","""{"acpdSource":"host-filesystem","acpdWrite":"changed-without-consent"}"""))
        val tool=changed.items.single() as TimelineItem.Tool
        assertEquals(HOST_WRITE_CHANGED,tool.hostWrite);assertEquals("failed",tool.status)
        // A later update without the flag keeps it.
        val later=SessionReducer.reduce(changed,toolUpdate(3,"tool_call_update","failed","""{}"""))
        assertEquals(HOST_WRITE_CHANGED,(later.items.single() as TimelineItem.Tool).hostWrite)
        val unchanged=SessionReducer.reduce(SessionState(),toolUpdate(1,"tool_call","completed","""{"acpdSource":"host-filesystem","acpdWrite":"unchanged"}"""))
        assertEquals(HOST_WRITE_UNCHANGED,(unchanged.items.single() as TimelineItem.Tool).hostWrite)
        // An agent cannot forge a host notice, and unknown values are ignored.
        val forged=SessionReducer.reduce(SessionState(),toolUpdate(1,"tool_call","failed","""{"acpdWrite":"changed-without-consent"}""","agent"))
        assertNull((forged.items.single() as TimelineItem.Tool).hostWrite)
        val unknown=SessionReducer.reduce(SessionState(),toolUpdate(1,"tool_call","failed","""{"acpdWrite":"something-else"}"""))
        assertNull((unknown.items.single() as TimelineItem.Tool).hostWrite)
        assertEquals("File changed without your approval",hostWriteLabel(HOST_WRITE_CHANGED))
        assertEquals("Already up to date · nothing written",hostWriteLabel(HOST_WRITE_UNCHANGED))
        assertNull(hostWriteLabel("x"));assertNull(hostWriteExplanation("x"))
        assertTrue(hostWriteExplanation(HOST_WRITE_CHANGED)!!.contains("did not approve"))
        assertTrue(transcriptMarkdown(SessionInfo("1","agent","s","/w"),changed).contains("Host: File changed without your approval"))
        // Older saved timelines decode without the field.
        val saved=WireJson.decodeFromString<TimelineItem>("""{"type":"dev.acportal.protocol.TimelineItem.Tool","id":"t","title":"Old"}""") as TimelineItem.Tool
        assertNull(saved.hostWrite)
    }

    @Test fun ownToolsFlagIsReadFromDiscoveryAndSessionMetadataAndDefaultsOff() {
        assertTrue(WireJson.decodeFromString<AgentInfo>("""{"id":"opencode","name":"OpenCode","installed":true,"status":"available","ownTools":true}""").ownTools)
        assertFalse(WireJson.decodeFromString<AgentInfo>("""{"id":"old","name":"Old","installed":true,"status":"available"}""").ownTools)
        assertTrue(WireJson.decodeFromString<SessionInfo>("""{"id":"1","agentId":"opencode","acpSessionId":"s","workspace":"/w","ownTools":true}""").ownTools)
        assertFalse(WireJson.decodeFromString<SessionInfo>("""{"id":"1","agentId":"goose","acpSessionId":"s","workspace":"/w"}""").ownTools)
        // The warning names what the host enforces, without claiming to see the agent's actions.
        assertTrue(OWN_TOOLS_WARNING.contains("do not go through this host's approval prompts"))
    }

    @Test fun hostFeaturesAreOptional() {
        assertEquals(listOf(HOST_FEATURE_FORM_ELICITATION),WireJson.decodeFromString<HostStatus>("""{"hostId":"h","name":"n","version":"0.1","features":["formElicitation"]}""").features)
        assertTrue(WireJson.decodeFromString<HostStatus>("""{"hostId":"h","name":"n","version":"0.1"}""").features.isEmpty())
    }
}
