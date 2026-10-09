package dev.acportal.presentation

import dev.acportal.protocol.*
import dev.acportal.storage.StoredSession
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class SessionListSummaryTest {
    private val info = SessionInfo("s", "goose", "acp", "/home/me/project")
    private fun row(id: String, state: SessionState, updatedAt: Long = 1) =
        StoredSession("h", id, WireJson.encodeToString(info.copy(id = id)), WireJson.encodeToString(state), updatedAt = updatedAt)

    @Test fun titleFallsBackFromAgentTitleToFirstUserLine() {
        val titled = SessionState(sessionInfo = buildJsonObject { put("title", "Fix the build\nmore") })
        assertEquals("Fix the build", sessionListSummary(row("a", titled)).title)
        val untitled = SessionState(items = listOf(TimelineItem.Text("1", "agent", "hello"), TimelineItem.Text("2", "user", "Rename files\nplease")))
        assertEquals("Rename files", sessionListSummary(row("b", untitled)).title)
        assertEquals("Agent session", sessionListSummary(row("c", SessionState())).title)
    }

    @Test fun searchUsesConversationTextNotStoredJson() {
        val state = SessionState(sequence = 9, items = listOf(TimelineItem.Text("1", "user", "Deploy staging")))
        val search = sessionListSummary(row("a", state)).searchText
        assertTrue(search.contains("deploy staging", ignoreCase = true))
        assertTrue(search.contains("/home/me/project"))
        assertTrue(search.contains("goose"))
        // The raw JSON keys (role, sequence, items) no longer make every session match.
        assertFalse(search.contains("sequence"))
        assertFalse(search.contains("role"))
    }

    @Test fun searchTextIsBounded() {
        val state = SessionState(items = List(40) { TimelineItem.Text("$it", "agent", "x".repeat(4096)) })
        assertTrue(sessionListSummary(row("a", state)).searchText.length <= SESSION_SEARCH_MAX_CHARS + 64)
    }

    @Test fun unreadableStateStillSummarises() {
        val summary = sessionListSummary(StoredSession("h", "a", "", "{broken", updatedAt = 5))
        assertEquals("Agent session", summary.title)
        assertEquals(5L, summary.activity?.timestamp)
    }

    @Test fun onlyChangedRowsAreDecodedAgain() {
        val memo = SessionListSummaries()
        val rows = (1..20).map { row("s$it", SessionState(items = listOf(TimelineItem.Text("1", "user", "task $it")))) }
        memo.update(rows)
        assertEquals(20, memo.decoded)
        // Room hands back fresh copies of every row when one of them saves.
        val next = rows.map { it.copy(state = String(it.state.toCharArray())) }.toMutableList()
        next[3] = row("s4", SessionState(items = listOf(TimelineItem.Text("1", "user", "renamed"))), updatedAt = 2)
        val summaries = memo.update(next)
        assertEquals(21, memo.decoded)
        assertEquals("renamed", summaries.getValue("h/s4").title)
        assertEquals("task 5", summaries.getValue("h/s5").title)
        // Removed rows are dropped from the memo.
        assertEquals(1, memo.update(next.take(1)).size)
        assertEquals(21, memo.decoded)
    }
}
