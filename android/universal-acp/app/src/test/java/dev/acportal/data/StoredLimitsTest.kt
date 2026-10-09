package dev.acportal.data

import dev.acportal.protocol.*
import dev.acportal.storage.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class StoredLimitsTest {
    private fun bytes(text: String) = text.toByteArray(Charsets.UTF_8).size

    @Test fun oneRowStaysBelowTheCursorWindow() {
        // Android's default CursorWindow holds 2 MiB; one session row must fit with room for keys.
        assertTrue(STORED_STATE_MAX_BYTES >= MAX_SESSION_CACHE_BYTES)
        assertTrue(STORED_METADATA_MAX_BYTES + STORED_STATE_MAX_BYTES + STORED_DRAFT_MAX_BYTES <= 2 * 1024 * 1024 - 256 * 1024)
        assertTrue(STORED_DRAFT_MAX_BYTES >= MAX_PROMPT_WIRE_BYTES)
    }

    @Test fun utf8LengthMatchesTheEncoder() {
        for (text in listOf("", "abc", "é界", "😀x")) assertEquals(text, bytes(text), utf8Bytes(text))
        // A lone surrogate is counted as three bytes, never less than any encoder writes.
        for (text in listOf("a\uD800b", "\uDC00")) assertTrue(utf8Bytes(text) >= bytes(text))
        assertTrue(utf8Bytes("a".repeat(10_000), 10) <= 12)
    }

    @Test fun shortDraftsAreKeptAndLongDraftsCutAtACharacterBoundary() {
        assertEquals("hello", storedDraft("hello"))
        val ascii = "a".repeat(STORED_DRAFT_MAX_BYTES + 10)
        assertEquals(STORED_DRAFT_MAX_BYTES, bytes(storedDraft(ascii)))
        val emoji = "😀".repeat(STORED_DRAFT_MAX_BYTES / 4 + 5)
        val cut = storedDraft(emoji)
        assertTrue(bytes(cut) <= STORED_DRAFT_MAX_BYTES)
        assertFalse(Character.isHighSurrogate(cut.last()))
        assertTrue(emoji.startsWith(cut))
        val mixed = "a" + "界".repeat(STORED_DRAFT_MAX_BYTES / 3 + 1)
        assertEquals(1 + 3 * (STORED_DRAFT_MAX_BYTES / 3), bytes(storedDraft(mixed)))
    }

    private fun info(setup: JsonObject = JsonObject(emptyMap()), extra: String = "") = SessionInfo(
        "s1", "agent", "acp-1", "/w",
        initialization = buildJsonObject {
            put("agentCapabilities", buildJsonObject { put("loadSession", true); put("huge", extra) })
            put("agentInfo", buildJsonObject { put("name", "Goose") })
        },
        setup = setup,
        workspaceAccess = WorkspaceAccess(),
    )

    @Test fun smallMetadataIsStoredUnchanged() {
        val value = info(setup = buildJsonObject { put("modes", "m") })
        assertEquals(WireJson.encodeToString(value), storedMetadata(value))
    }

    @Test fun oversizedSetupIsDroppedFirst() {
        val value = info(setup = buildJsonObject { put("models", "x".repeat(STORED_METADATA_MAX_BYTES)) })
        val stored = storedMetadata(value)
        assertTrue(bytes(stored) <= STORED_METADATA_MAX_BYTES)
        val restored = WireJson.decodeFromString<SessionInfo>(stored)
        assertTrue(restored.setup.isEmpty())
        assertEquals(value.initialization, restored.initialization)
        assertEquals(value.workspace, restored.workspace)
    }

    @Test fun oversizedInitializationKeepsOnlyWhatOfflinePagesUse() {
        val value = info(extra = "界".repeat(STORED_METADATA_MAX_BYTES))
        val stored = storedMetadata(value)
        assertTrue(bytes(stored) <= STORED_METADATA_MAX_BYTES)
        val restored = WireJson.decodeFromString<SessionInfo>(stored)
        assertEquals(true, restored.initialization["agentCapabilities"]!!.jsonObject["loadSession"]!!.jsonPrimitive.boolean)
        assertEquals("Goose", restored.initialization["agentInfo"]!!.jsonObject["name"]!!.jsonPrimitive.content)
        assertNull(restored.initialization["agentCapabilities"]!!.jsonObject["huge"])
        assertEquals("acp-1", restored.acpSessionId)
    }

    @Test fun unreadableOrGuardedMetadataShowsAsUnavailable() {
        for (metadata in listOf("", "{not json", """{"id":"s1"}""")) {
            val restored = storedSessionInfo(StoredSession("h", "s1", metadata))
            assertEquals("s1", restored.id)
            assertEquals(STORED_DETAILS_UNAVAILABLE, restored.status)
        }
        val good = info()
        assertEquals(good, storedSessionInfo(StoredSession("h", "s1", WireJson.encodeToString(good))))
    }

    @Test fun unreadableOrGuardedStateBecomesAHistoryGap() {
        assertEquals(SessionState(), storedSessionState(StoredSession("h", "s1", "", state = "")))
        assertTrue(storedSessionState(StoredSession("h", "s1", "", state = "{broken")).historyGap)
        val placeholder = storedSessionState(StoredSession("h", "s1", "", state = OVERSIZED_STATE_PLACEHOLDER))
        assertTrue(placeholder.historyGap)
        assertEquals(0L, placeholder.sequence)
        assertTrue(placeholder.items.isEmpty())
        val saved = SessionState(sequence = 4, items = listOf(TimelineItem.Text("m", "agent", "hi")))
        assertEquals(saved, storedSessionState(StoredSession("h", "s1", "", state = WireJson.encodeToString(saved))))
    }
}
