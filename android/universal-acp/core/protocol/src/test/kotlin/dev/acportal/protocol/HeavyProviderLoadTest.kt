package dev.acportal.protocol

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test
import java.lang.management.ManagementFactory

/**
 * Retention and allocation under heavier provider load than the ordinary reducer tests:
 * long streams, many tool calls and very large model lists. Allocation is measured per
 * thread on the JVM; the Android heap itself is profiled on a device (emulator runbook §2).
 */
class HeavyProviderLoadTest {
    private val threads = ManagementFactory.getThreadMXBean() as com.sun.management.ThreadMXBean
    private fun allocated() = threads.getThreadAllocatedBytes(Thread.currentThread().id)
    private fun event(sequence: Int, update: String) =
        WireJson.parseToJsonElement("""{"type":"event","sequence":$sequence,"direction":"agent","message":{"method":"session/update","params":{"update":$update}}}""").objectValue()
    private fun chunk(sequence: Int, text: String, messageId: String = "m") =
        event(sequence, """{"sessionUpdate":"agent_message_chunk","messageId":"$messageId","content":{"type":"text","text":"$text"}}""")
    private fun tool(sequence: Int, size: Int) =
        event(sequence, """{"sessionUpdate":"tool_call","toolCallId":"t$sequence","title":"Read","content":[{"type":"content","content":{"type":"text","text":"${"x".repeat(size)}"}}]}""")

    @Test fun aLongStreamKeepsOnlyTheNewestMebibyteOfOneMessage() {
        var state = SessionState()
        val piece = "s".repeat(200)
        for (i in 1..8_000) state = SessionReducer.reduce(state, chunk(i, piece))
        val message = state.items.single() as TimelineItem.Text
        assertEquals(1024 * 1024, message.text.length)
        assertTrue(state.historyGap)
    }

    @Test fun manyToolCallsStayWithinTheItemAndByteBudgets() {
        var state = SessionState()
        for (i in 1..3_000) state = SessionReducer.reduce(state, tool(i, 1_000))
        assertTrue(state.items.size <= 2_000)
        assertTrue(state.historyGap)
        assertEquals("t3000", state.items.last().id)
        val retainedChars = state.items.sumOf { (it as TimelineItem.Tool).contentChars }
        assertTrue("retained ${retainedChars * 3} bytes", retainedChars * 3 <= 8L * 1024 * 1024)
    }

    @Test fun chunkCostDoesNotGrowWithEarlierToolHistory() {
        fun perChunk(history: Int): Long {
            var state = SessionState()
            for (i in 1..history) state = SessionReducer.reduce(state, tool(i, 200))
            for (i in 1..50) state = SessionReducer.reduce(state, chunk(history + i, "warm "))
            val before = allocated()
            for (i in 1..400) state = SessionReducer.reduce(state, chunk(history + 50 + i, "word "))
            return (allocated() - before) / 400
        }
        val small = perChunk(100)
        val large = perChunk(1_900)
        // A streamed chunk copies the item list once and walks it for retention, so cost
        // grows with item count but must stay far below re-serialising the history
        // (~1.9k tools x 200 chars x 3 bytes ≈ 1.1 MB per chunk).
        assertTrue("per chunk with 1.9k items: $large bytes", large < 256 * 1024)
        assertTrue("small history $small bytes, large $large bytes", large < small + 192 * 1024)
    }

    @Test fun aVeryLargeModelListIsRefusedWhileAnOrdinaryLargeOneIsKept() {
        fun models(count: Int) = buildJsonObject {
            put("currentModelId", "m0")
            putJsonArray("availableModels") { repeat(count) { add(buildJsonObject { put("modelId", "provider/model-$it"); put("name", "Model number $it with a descriptive name") }) } }
        }
        val ordinary = models(5_000)
        val kept = SessionReducer.limitMetadata(SessionState(), SessionState(models = ordinary))
        assertSame(ordinary, kept.models)
        assertNull(kept.error)
        val huge = models(40_000)
        val refused = SessionReducer.limitMetadata(kept, kept.copy(models = huge))
        assertSame(ordinary, refused.models)
        assertNotNull(refused.error)
        assertTrue(refused.historyGap)
    }
}
