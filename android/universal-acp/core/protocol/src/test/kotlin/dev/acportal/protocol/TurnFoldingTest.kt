package dev.acportal.protocol

import org.junit.Assert.*
import org.junit.Test

class TurnFoldingTest {
    private fun user(id: String) = TimelineItem.Text(id, "user", "q")
    private fun agent(id: String) = TimelineItem.Text(id, "agent", "a")
    private fun thought(id: String) = TimelineItem.Text(id, "thought", "t")
    private fun tool(id: String) = TimelineItem.Tool(id, "read", "read", "completed")

    @Test fun aFinishedTurnFoldsEverythingBeforeItsFinalReply() {
        val blocks = foldTurns(listOf(user("u1"), thought("t1"), tool("a"), tool("b"), agent("r1")), processing = false, waiting = false)
        assertEquals(3, blocks.size)
        assertTrue(blocks[0] is TurnBlock.Row)
        val steps = blocks[1] as TurnBlock.Steps
        assertEquals("u1", steps.id)
        assertEquals(3, steps.items.size)
        assertEquals(2, steps.tools)
        assertEquals("steps:u1", steps.key)
        assertEquals("r1", (blocks[2] as TurnBlock.Row).item.id)
    }
    @Test fun theRunningTurnStaysOpenAndEarlierTurnsFold() {
        val items = listOf(user("u1"), tool("a"), agent("r1"), user("u2"), tool("b"), thought("t2"))
        val blocks = foldTurns(items, processing = true, waiting = false)
        assertEquals(listOf("u1", "steps:u1", "r1", "u2", "b", "t2"), blocks.map { it.key })
    }
    @Test fun aTurnWaitingForAnAnswerIsNotFoldedEvenWhenNothingRuns() {
        val items = listOf(user("u1"), tool("a"), agent("r1"))
        assertFalse(foldTurns(items, processing = false, waiting = true).any { it is TurnBlock.Steps })
        assertTrue(foldTurns(items, processing = false, waiting = false).any { it is TurnBlock.Steps })
    }
    @Test fun noFinalReplyAnErrorOrNothingBeforeItMeansNoFold() {
        assertFalse(foldTurns(listOf(user("u"), tool("a")), false, false).any { it is TurnBlock.Steps })
        assertFalse(foldTurns(listOf(user("u"), agent("r")), false, false).any { it is TurnBlock.Steps })
        val failed = listOf(user("u"), TimelineItem.Notice("n", "Prompt failed", error = true), agent("r"))
        assertFalse(foldTurns(failed, false, false).any { it is TurnBlock.Steps })
    }
    @Test fun earlierAgentRemarksFoldAndOnlyTheLastAgentMessageStays() {
        val blocks = foldTurns(listOf(user("u"), agent("first"), tool("a"), agent("last")), false, false)
        assertEquals(listOf("u", "steps:u", "last"), blocks.map { it.key })
        assertEquals(1, (blocks[1] as TurnBlock.Steps).tools)
        assertEquals("thinking", stepsLabel(TurnBlock.Steps("x", listOf(thought("t")), 0)))
        assertEquals("1 step", stepsLabel(TurnBlock.Steps("x", listOf(tool("a")), 1)))
    }
    @Test fun itemsBeforeTheFirstUserMessageAreKept() {
        val blocks = foldTurns(listOf(agent("early"), user("u"), agent("r")), false, false)
        assertEquals(listOf("early", "u", "r"), blocks.map { it.key })
    }
    @Test fun durationsReadShort() {
        assertEquals("45s", formatDuration(45_000))
        assertEquals("3m 05s", formatDuration(185_000))
        assertEquals("1h 12m", formatDuration(4_320_000))
        assertEquals("3m", briefDuration(185_000))
        assertEquals("45s", briefDuration(45_000))
        assertEquals(72_000L, workedMillis(mapOf("a" to TurnTime(0, 30_000), "b" to TurnTime(1, 42_000), "c" to TurnTime(2))))
        assertEquals(0L, workedMillis(null))
    }
}

class TurnClockTest {
    @Test fun aTurnIsTimedOnceAndRepeatsChangeNothing() {
        var all: TurnTimes = emptyMap()
        all = TurnClock.start(all, "h/s", "u1", 1_000)
        assertSame(all, TurnClock.start(all, "h/s", "u1", 9_999))
        all = TurnClock.end(all, "h/s", "u1", 73_000)
        assertEquals(72_000L, all["h/s"]!!["u1"]!!.took)
        assertSame(all, TurnClock.end(all, "h/s", "u1", 99_000))
        assertSame(all, TurnClock.end(all, "h/s", "unknown", 99_000))
        assertEquals(72_000L, workedMillis(all["h/s"]))
    }
    @Test fun storageStaysBounded() {
        var all: TurnTimes = emptyMap()
        for (s in 0 until TurnClock.MAX_SESSIONS + 5) all = TurnClock.start(all, "h/$s", "u", s.toLong())
        assertEquals(TurnClock.MAX_SESSIONS, all.size)
        assertFalse("h/0" in all)
        for (t in 0 until TurnClock.MAX_TURNS + 10) all = TurnClock.start(all, "h/new", "u$t", 10_000L + t)
        assertEquals(TurnClock.MAX_TURNS, all["h/new"]!!.size)
    }
}
