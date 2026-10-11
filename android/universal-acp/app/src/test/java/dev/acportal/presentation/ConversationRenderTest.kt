package dev.acportal.presentation

import dev.acportal.protocol.*
import dev.acportal.storage.StoredSession
import kotlinx.serialization.encodeToString
import org.junit.Assert.*
import org.junit.Test
import java.lang.management.ManagementFactory

/** Per-update work done by the conversation screen while a reply streams (runbook §8). */
class ConversationRenderTest {
    private val threads = ManagementFactory.getThreadMXBean() as com.sun.management.ThreadMXBean
    private fun allocated() = threads.getThreadAllocatedBytes(Thread.currentThread().id)
    private val history: List<TimelineItem> = buildList {
        add(TimelineItem.Text("u0", "user", "Refactor the parser\nand add tests"))
        repeat(1_999) { add(if (it % 2 == 0) TimelineItem.Tool("t$it", "Read") else TimelineItem.Text("a$it", "agent", "x".repeat(4_000))) }
    }

    @Test fun titleAndTailKeyAreComputedWithoutCopyingTheTimeline() {
        repeat(200) { firstUserLine(history); timelineTailKey(history); lastUserText(history) }
        val before = allocated()
        repeat(1_000) { firstUserLine(history); timelineTailKey(history) }
        val perFrame = (allocated() - before) / 1_000
        // filterIsInstance on 2,000 items allocated a list of ~1,000 references (≥ 4 KB) per frame.
        assertTrue("allocated $perFrame bytes per frame", perFrame < 1_024)
        assertEquals("Refactor the parser", firstUserLine(history))
        assertNull(firstUserLine(listOf(TimelineItem.Text("a", "agent", "only agent"))))
    }

    @Test fun tailKeyChangesWhenTheLastMessageGrowsOrANewItemArrives() {
        val streaming = history + TimelineItem.Text("live", "agent", "Hel")
        val grown = history + TimelineItem.Text("live", "agent", "Hello")
        assertNotEquals(timelineTailKey(streaming), timelineTailKey(grown))
        assertNotEquals(timelineTailKey(grown), timelineTailKey(grown + TimelineItem.Tool("next", "Edit")))
        assertEquals(timelineTailKey(grown), timelineTailKey(grown.toList()))
    }

    @Test fun lastUserTextSearchesFromTheEnd() {
        val items = history + TimelineItem.Text("u1", "user", "second prompt") + TimelineItem.Text("r", "agent", "reply")
        assertEquals("second prompt", lastUserText(items))
        assertNull(lastUserText(emptyList()))
    }

    @Test fun markdownKeepsThePreviousRenderingOnlyWhileTextIsAppended() {
        assertNull(markdownToShow<String>(null, "a"))
        assertEquals("doc", markdownToShow("# Title" to "doc", "# Title"))
        assertEquals("doc", markdownToShow("# Title" to "doc", "# Title and more"))
        assertNull("an edited message must not show a different rendering", markdownToShow("# Title" to "doc", "# Other"))
        assertNull(markdownToShow("# Title and more" to "doc", "# Title"))
        assertNull("a failed parse falls back to plain text", markdownToShow<String>("# Title" to null, "# Title more"))
    }

    @Test fun manySavedSessionsDecodeOnceAndOnlyChangedRowsAfterwards() {
        val info = SessionInfo("s", "goose", "acp", "/home/me/project")
        val rows = (1..300).map { index ->
            val state = SessionState(items = listOf(TimelineItem.Text("u", "user", "Task $index"), TimelineItem.Text("a", "agent", "y".repeat(16_000))))
            StoredSession("h", "s$index", WireJson.encodeToString(info.copy(id = "s$index")), WireJson.encodeToString(state), updatedAt = index.toLong())
        }
        val memo = SessionListSummaries()
        assertEquals(300, memo.update(rows).size)
        assertEquals(300, memo.decoded)
        memo.update(rows)
        assertEquals(300, memo.decoded)
        val changed = rows.toMutableList().also { it[7] = it[7].copy(state = WireJson.encodeToString(SessionState(items = listOf(TimelineItem.Text("u", "user", "Renamed task"))))) }
        assertEquals("Renamed task", memo.update(changed).getValue("h/s8").title)
        assertEquals(301, memo.decoded)
        assertEquals(1, memo.update(changed.take(1)).size)
    }
}
