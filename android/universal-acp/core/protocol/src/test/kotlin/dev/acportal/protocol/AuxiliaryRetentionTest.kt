package dev.acportal.protocol

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class AuxiliaryRetentionTest {
    private fun permissionMessage(id: String, text: String = "review") = buildJsonObject {
        put("id", id); put("method", "session/request_permission")
        putJsonObject("params") { put("description", text); putJsonArray("options") {} }
    }

    @Test fun pendingCountOverflowRejectsWholeEventWithoutDroppingEarlierConsent() {
        var state = SessionState()
        repeat(128) { index ->
            state = SessionReducer.reduce(state, buildJsonObject {
                put("type", "event"); put("sequence", index + 1L); put("message", permissionMessage("p-$index"))
            })
        }
        val accepted = state
        assertThrows(PendingPermissionLimitException::class.java) {
            SessionReducer.reduce(accepted, buildJsonObject {
                put("type", "event"); put("sequence", 129); put("message", permissionMessage("overflow"))
            })
        }
        assertEquals(128, accepted.permissions.size)
        assertEquals(128L, accepted.sequence)
        assertFalse(accepted.permissions.containsKey("\"overflow\""))
    }

    @Test fun replayPermissionByteOverflowRejectsBeforeSnapshotBecomesActionable() {
        var state = SessionReducer.reduce(SessionState(), buildJsonObject { put("type", "replay_start") })
        repeat(6) { index ->
            state = SessionReducer.reduce(state, buildJsonObject {
                put("type", "pending_permission"); put("message", permissionMessage("p-$index", "x".repeat(400000)))
            })
        }
        val accepted = state
        assertThrows(PendingPermissionLimitException::class.java) {
            SessionReducer.reduce(accepted, buildJsonObject {
                put("type", "pending_permission"); put("message", permissionMessage("overflow", "x".repeat(400000)))
            })
        }
        assertTrue(accepted.replaying)
        assertEquals(6, accepted.replayPermissions!!.size)
        assertTrue(accepted.permissions.isEmpty())
    }
    @Test fun terminalCountEvictionReportsGap() {
        var state = SessionState()
        repeat(17) { index ->
            state = SessionReducer.reduce(state, event(index + 1L, buildJsonObject {
                put("sessionUpdate", "tool_call_update"); put("toolCallId", "tool-$index")
                putJsonObject("_meta") { putJsonObject("acpdTerminal") { put("terminalId", "terminal-$index"); put("output", "ok") } }
            }, "host"))
        }
        assertEquals(16, state.terminals.size)
        assertFalse(state.terminals.containsKey("terminal-0"))
        assertTrue(state.historyGap)
    }
    private fun event(sequence: Long, update: JsonObject, direction: String = "agent") = buildJsonObject {
        put("type", "event"); put("sequence", sequence); put("direction", direction)
        putJsonObject("message") { put("method", "session/update"); putJsonObject("params") { put("update", update) } }
    }

    @Test fun accumulatingMetadataRejectsOversizeUpdateAndPreservesPriorFields() {
        var state = SessionState(sessionInfo = buildJsonObject { put("title", "Keep this title") })
        state = SessionReducer.reduce(state, event(1, buildJsonObject { put("sessionUpdate", "session_info_update"); put("first", "x".repeat(400000)) }))
        val accepted = state.sessionInfo
        state = SessionReducer.reduce(state, event(2, buildJsonObject { put("sessionUpdate", "session_info_update"); put("second", "y".repeat(400000)) }))
        assertEquals(accepted, state.sessionInfo)
        assertEquals(2L, state.sequence)
        assertTrue(state.historyGap)
        assertEquals(SessionErrorOrigin.PROTOCOL, state.errorOrigin)
        assertTrue(state.error!!.contains("metadata"))
    }

    @Test fun terminalBytePressureRetainsNewestSnapshotWithoutRemovingPendingConsent() {
        val permission = Permission(JsonPrimitive("approval"), buildJsonObject { putJsonArray("options") {} })
        var state = SessionState(permissions = mapOf("approval" to permission))
        for (index in 1..2) {
            state = SessionReducer.reduce(state, event(index.toLong(), buildJsonObject {
                put("sessionUpdate", "tool_call_update"); put("toolCallId", "tool-$index")
                putJsonObject("_meta") { putJsonObject("acpdTerminal") { put("terminalId", "terminal-$index"); put("output", "x".repeat(400000)) } }
            }, "host"))
        }
        assertEquals(setOf("terminal-2"), state.terminals.keys)
        assertTrue(state.historyGap)
        assertEquals(permission, state.permissions["approval"])
        assertEquals(400000, state.terminals.getValue("terminal-2")["output"].text().length)
    }
}
