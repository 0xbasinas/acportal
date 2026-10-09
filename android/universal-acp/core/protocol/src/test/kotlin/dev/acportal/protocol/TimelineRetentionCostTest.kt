package dev.acportal.protocol

import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class TimelineRetentionCostTest {
    private fun event(sequence: Int, message: String) =
        WireJson.parseToJsonElement("""{"type":"event","sequence":$sequence,"direction":"agent","message":$message}""").objectValue()

    @Test fun streamingUpdatesDoNotReserialiseEarlierItems() {
        var state = SessionState()
        for (i in 1..200) state = SessionReducer.reduce(state, event(i,
            """{"method":"session/update","params":{"update":{"sessionUpdate":"tool_call","toolCallId":"t$i","title":"Read","content":[{"type":"content","content":{"type":"text","text":"${"x".repeat(2000)}"}}]}}}"""))
        assertEquals(200, state.items.size)
        val before = timelineSizeComputations.get()
        for (i in 201..300) state = SessionReducer.reduce(state, event(i,
            """{"method":"session/update","params":{"update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"chunk $i "}}}}"""))
        // Text chunks are sized from string lengths; the 200 tool items are not serialised again.
        assertEquals(0L, timelineSizeComputations.get() - before)
    }

    @Test fun cachedSizesStayOutOfStorageAndEquality() {
        val tool = TimelineItem.Tool("t", "Read", content = buildJsonArray { add("x") })
        assertTrue(tool.contentChars > 0)
        val encoded = WireJson.encodeToString<TimelineItem>(tool)
        assertFalse(encoded.contains("contentChars"))
        assertEquals(tool, WireJson.decodeFromString<TimelineItem>(encoded))
        assertEquals(tool, tool.copy())
        val changed = tool.copy(content = buildJsonArray { add("longer") })
        assertTrue(changed.contentChars > tool.contentChars)
    }

    @Test fun retentionStillOmitsOldestItemsPastTheByteBudget() {
        var state = SessionState()
        val big = "y".repeat(400_000)
        for (i in 1..12) state = SessionReducer.reduce(state, event(i,
            """{"method":"session/update","params":{"update":{"sessionUpdate":"tool_call","toolCallId":"t$i","title":"Read","content":[{"type":"content","content":{"type":"text","text":"$big"}}]}}}"""))
        assertTrue(state.historyGap)
        assertTrue(state.items.size < 12)
        assertEquals("t12", (state.items.last() as TimelineItem.Tool).id)
    }
}
