package dev.acportal.protocol

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class SessionReducerTest {
    @Test fun reconnectClearsOnlyConnectionErrorsAndPreservesPendingTurn() {
        val initial=SessionState(processing=true,promptRequestId="pending",error="Earlier agent failure")
        val replay=WireJson.parseToJsonElement("""{"type":"replay_complete","latestSequence":4}""").objectValue()
        assertEquals("Earlier agent failure",SessionReducer.reduce(initial,replay).error)
        val disconnected=SessionReducer.reduce(initial,WireJson.parseToJsonElement("""{"type":"reconnect_required"}""").objectValue())
        assertEquals(SessionErrorOrigin.CONNECTION,disconnected.errorOrigin)
        val recovered=SessionReducer.reduce(disconnected,replay)
        assertNull(recovered.error);assertTrue(recovered.processing);assertEquals("pending",recovered.promptRequestId)
    }
    @Test fun agentFailuresAndClosedSessionsHaveSeparateRecovery() {
        val failed=SessionReducer.reduce(SessionState(processing=true,promptRequestId="\"p\""),event(1,"""{"id":"p","error":{"code":-32603,"message":"Provider declined"}}"""))
        assertEquals(SessionErrorOrigin.AGENT,failed.errorOrigin);assertEquals("Provider declined",failed.error);assertFalse(failed.processing)
        val closed=SessionReducer.reduce(failed,WireJson.parseToJsonElement("""{"type":"session_closed","reason":"Process exited"}""").objectValue())
        assertEquals(SessionErrorOrigin.SESSION,closed.errorOrigin)
        assertTrue(closed.copy(error=null).sessionClosed)
        val legacy=WireJson.decodeFromString<SessionState>("""{"error":"Legacy failure"}""")
        assertEquals(SessionErrorOrigin.AGENT,legacy.errorOrigin)
    }
    @Test fun configurationFailureDoesNotFinishAnUnrelatedPrompt() {
        val state=SessionReducer.reduce(SessionState(),event(1,"""{"id":"prompt","method":"session/prompt","params":{"prompt":[]}}""","client"))
        val rejected=SessionReducer.reduce(state,event(2,"""{"id":"configuration","error":{"code":-1,"message":"Choice rejected"}}"""))
        assertTrue(rejected.processing)
        val failed=SessionReducer.reduce(rejected,event(3,"""{"id":"prompt","error":{"code":-1,"message":"Prompt failed"}}"""))
        assertFalse(failed.processing)
        assertNull(failed.promptRequestId)
    }
    @Test fun configurationResponseReplacesTheCompleteDependentState() {
        val state=SessionState(configOptions=WireJson.parseToJsonElement("""[{"id":"old"}]""").arrayValue())
        val next=SessionReducer.reduce(state,event(1,"""{"id":"config","result":{"configOptions":[{"id":"engine","currentValue":"new"},{"id":"reasoning","currentValue":"high"}]}}"""))
        assertEquals(listOf("engine","reasoning"),next.configOptions.map {it.objectValue()["id"].text()})
    }
    @Test fun modelSelectionChangesOnlyAfterItsSuccessfulResponse() {
        val state=SessionState(models=WireJson.parseToJsonElement("""{"currentModelId":"old"}""").objectValue())
        val requested=SessionReducer.reduce(state,event(1,"""{"id":"select","method":"session/set_model","params":{"modelId":"new"}}""","client"))
        assertEquals("old",requested.models["currentModelId"].text())
        val denied=SessionReducer.reduce(requested,event(2,"""{"id":"select","error":{"code":-1,"message":"Rejected"}}"""))
        assertEquals("old",denied.models["currentModelId"].text())
        assertTrue(denied.modelRequests.isEmpty())
        val accepted=SessionReducer.reduce(requested,event(2,"""{"id":"select","result":{}}"""))
        assertEquals("new",accepted.models["currentModelId"].text())
        assertTrue(accepted.modelRequests.isEmpty())
    }
    @Test fun terminalSnapshotsPatchToolsAndRemainAvailableAfterRelease() {
        val first=event(1,"""{"method":"session/update","params":{"update":{"sessionUpdate":"tool_call","toolCallId":"tool","title":"Run tests","content":[{"type":"terminal","terminalId":"terminal"}]}}}""","host")
        val patch=event(2,"""{"method":"session/update","params":{"update":{"sessionUpdate":"tool_call_update","toolCallId":"tool","status":"completed","_meta":{"acpdTerminal":{"terminalId":"terminal","output":"all tests passed","truncated":false,"exitStatus":{"exitCode":0}}}}}}""","host")
        val state=SessionReducer.reduce(SessionReducer.reduce(SessionState(),first),patch)
        assertEquals("all tests passed",state.terminals["terminal"]?.get("output").text())
        assertEquals("completed",(state.items.single() as TimelineItem.Tool).status)
        assertEquals(state,SessionReducer.reduce(state,patch))
    }
    @Test fun authenticationChoicesExcludeTerminalAndFutureFlows() {
        val info=SessionInfo("id","any-agent","","workspace",initialization=WireJson.parseToJsonElement("""{"authMethods":[{"id":"default","name":"Default"},{"id":"agent","type":"agent"},{"id":"terminal","type":"terminal"},{"id":"future","type":"future"},{"name":"Missing ID"}]}""").objectValue())
        assertEquals(listOf("default","agent"),info.authenticationMethods().map {it["id"].text()})
        assertTrue(info.copy(initialization=JsonObject(emptyMap())).authenticationMethods().isEmpty())
    }
    @Test fun hostAutoReviewIsKeptOnTheToolCardAndAgentClaimsAreIgnored() {
        val update="""{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call","toolCallId":"acpd-command-1","title":"Run shell command","kind":"execute","status":"in_progress","rawInput":{"shellLine":"python3 -m unittest -v"},"_meta":{"acpdAutoReview":{"decision":"allow","layer":"rules","reason":"read-only or test commands inside the workspace"}}}}}"""
        val host=SessionReducer.reduce(SessionState(),event(1,update,"host"))
        val tool=host.items.single() as TimelineItem.Tool
        assertEquals("allow",tool.autoReview!!["decision"].text())
        assertEquals("python3 -m unittest -v",tool.autoReview!!["shellLine"].text())
        assertEquals("Auto-allowed by rules",autoReviewLabel(tool.autoReview!!))
        val later=SessionReducer.reduce(host,event(2,"""{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call_update","toolCallId":"acpd-command-1","status":"completed"}}}""","host"))
        assertEquals("rules",(later.items.single() as TimelineItem.Tool).autoReview!!["layer"].text())
        val forged=SessionReducer.reduce(SessionState(),event(1,update))
        assertNull((forged.items.single() as TimelineItem.Tool).autoReview)
        assertEquals("Auto-denied by model",autoReviewLabel(buildJsonObject {put("decision","deny");put("layer","model")}))
        assertEquals("Sent to you by rules review",autoReviewLabel(buildJsonObject {put("decision","ask");put("layer","rules")}))
    }
    @Test fun oversizedAgentConfigurationKeepsThePreviousChoices() {
        val huge="x".repeat(800*1024)
        val state=SessionState(configOptions=WireJson.parseToJsonElement("""[{"id":"engine"}]""").arrayValue(),
            commands=WireJson.parseToJsonElement("""[{"name":"plan"}]""").arrayValue())
        val config=SessionReducer.reduce(state,event(1,"""{"method":"session/update","params":{"update":{"sessionUpdate":"config_option_update","configOptions":[{"id":"$huge"}]}}}"""))
        assertEquals("engine",config.configOptions.single().objectValue()["id"].text())
        assertEquals(SessionErrorOrigin.PROTOCOL,config.errorOrigin);assertTrue(config.historyGap);assertEquals(1L,config.sequence)
        val commands=SessionReducer.reduce(state,event(1,"""{"method":"session/update","params":{"update":{"sessionUpdate":"available_commands_update","availableCommands":[{"name":"$huge"}]}}}"""))
        assertEquals("plan",commands.commands.single().objectValue()["name"].text())
        val usage=SessionReducer.reduce(state,event(1,"""{"method":"session/update","params":{"update":{"sessionUpdate":"usage_update","note":"$huge"}}}"""))
        assertTrue(usage.usage.isEmpty());assertNotNull(usage.error)
        val response=SessionReducer.reduce(state,event(1,"""{"id":"config","result":{"configOptions":[{"id":"$huge"}]}}"""))
        assertEquals("engine",response.configOptions.single().objectValue()["id"].text())
        // A normal-sized update still replaces the field and leaves no error.
        val small=SessionReducer.reduce(config.copy(error=null),event(2,"""{"method":"session/update","params":{"update":{"sessionUpdate":"config_option_update","configOptions":[{"id":"reasoning"}]}}}"""))
        assertEquals("reasoning",small.configOptions.single().objectValue()["id"].text());assertNull(small.error)
        // Host setup metadata applied outside the reducer uses the same limit.
        val models=WireJson.parseToJsonElement("""{"availableModels":[{"modelId":"$huge"}],"currentModelId":"m"}""").objectValue()
        val setup=SessionReducer.limitMetadata(state,state.copy(models=models))
        assertTrue(setup.models.isEmpty());assertNotNull(setup.error)
        val tooLongModel=SessionReducer.reduce(state,event(1,"""{"id":"select","method":"session/set_model","params":{"modelId":"${"m".repeat(2000)}"}}""","client"))
        assertTrue(tooLongModel.modelRequests.isEmpty())
    }
    private fun event(sequence:Int,message:String,direction:String="agent")=WireJson.parseToJsonElement("""{"type":"event","sequence":$sequence,"direction":"$direction","message":$message}""").objectValue()
    @Test fun streamedChunksMergeAndReplayDuplicatesAreIgnored() {
        val one=event(1,"""{"method":"session/update","params":{"update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Hello "}}}}""")
        val two=event(2,"""{"method":"session/update","params":{"update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"world"}}}}""")
        val state=SessionReducer.reduce(SessionReducer.reduce(SessionState(),one),two)
        assertEquals("Hello world",(state.items.single() as TimelineItem.Text).text)
        assertEquals(state,SessionReducer.reduce(state,two))
    }
    @Test fun toolPatchKeepsUnspecifiedContent() {
        val first=event(1,"""{"method":"session/update","params":{"update":{"sessionUpdate":"tool_call","toolCallId":"t","title":"Inspect file","kind":"read","content":[{"type":"content","content":{"type":"text","text":"file output"}}]}}}""")
        val patch=event(2,"""{"method":"session/update","params":{"update":{"sessionUpdate":"tool_call_update","toolCallId":"t","status":"completed"}}}""")
        val tool=SessionReducer.reduce(SessionReducer.reduce(SessionState(),first),patch).items.single() as TimelineItem.Tool
        assertEquals("Inspect file",tool.title);assertEquals("completed",tool.status);assertEquals(1,tool.content.size)
    }
    @Test fun permissionSurvivesReconnectUntilClientAcknowledgment() {
        val permission=event(1,"""{"id":7,"method":"session/request_permission","params":{"toolCall":{"title":"Write file"},"options":[{"optionId":"yes","kind":"allow_once","name":"Allow"}]}}""")
        val state=SessionReducer.reduce(SessionState(),permission)
        assertEquals("Write file",state.permissions.values.single().title)
        val response=permissionResponse(state.permissions.values.single(),"yes")
        assertEquals("yes",response["result"].objectValue()["outcome"].objectValue()["optionId"].text())
        assertTrue(SessionReducer.reduce(state,event(2,response.toString(),"client")).permissions.isEmpty())
    }
    @Test(expected=IllegalArgumentException::class) fun unknownPermissionOptionIsRejected() {
        permissionResponse(Permission(JsonPrimitive(1),buildJsonObject {putJsonArray("options") {}}),"invented")
    }
    @Test fun futureUpdatesRemainVisibleWithoutBreakingConversation() {
        val state=SessionReducer.reduce(SessionState(),event(1,"""{"method":"session/update","params":{"update":{"sessionUpdate":"future_feature"}}}"""))
        assertEquals(1L,state.sequence);assertTrue(state.items.single() is TimelineItem.Notice)
        assertFalse(SessionInfo("1","a","s","/").supportsLoad())
    }
    @Test fun diffPreservesLineNumbersAndChanges() {
        val diff=unifiedDiff("first\nold\nlast","first\nnew\nlast")
        assertEquals(listOf("old"),diff.filter {it.kind==DiffKind.REMOVED}.map {it.text})
        assertEquals(listOf("new"),diff.filter {it.kind==DiffKind.ADDED}.map {it.text})
        assertEquals(3,diff.last().oldLine);assertEquals(3,diff.last().newLine)
    }
}
