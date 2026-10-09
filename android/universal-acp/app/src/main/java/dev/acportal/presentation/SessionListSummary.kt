package dev.acportal.presentation

import dev.acportal.data.storedSessionInfo
import dev.acportal.data.storedSessionState
import dev.acportal.protocol.*
import dev.acportal.storage.StoredSession
import java.security.MessageDigest

/** Characters of conversation text kept per row for the Sessions search. */
internal const val SESSION_SEARCH_MAX_CHARS = 16 * 1024

/** What one Sessions row shows and searches; the saved conversation copy itself is not kept. */
data class SessionListSummary(val title: String, val activity: SessionActivityTime?, val searchText: String)

fun sessionKey(stored: StoredSession) = "${stored.hostId}/${stored.id}"

fun sessionListSummary(stored: StoredSession, info: SessionInfo = storedSessionInfo(stored)): SessionListSummary {
    val cached = storedSessionState(stored)
    val texts = cached.items.asSequence().filterIsInstance<TimelineItem.Text>()
    val title = cached.sessionInfo["title"].text().takeIf { it.isNotBlank() }?.lineSequence()?.firstOrNull()?.take(64)
        ?: texts.firstOrNull { it.role == "user" }?.text?.lineSequence()?.firstOrNull()?.take(64)
        ?: "Agent session"
    val search = StringBuilder()
    for (part in sequenceOf(info.workspace, info.agentId, title) + texts.map { it.text }) {
        if (search.length >= SESSION_SEARCH_MAX_CHARS) break
        search.append(part, 0, minOf(part.length, SESSION_SEARCH_MAX_CHARS - search.length)).append('\n')
    }
    return SessionListSummary(title, sessionActivityTime(stored, cached), search.toString())
}

/**
 * Keeps one summary per saved session and decodes a row's conversation copy again only when
 * that row changed. Room re-emits every row whenever any session saves (about twice a second
 * while an agent works), so decoding all of them each time would repeat up to 1 MiB of JSON
 * per session. Not thread-safe: use from one collector.
 */
class SessionListSummaries {
    private data class Version(val updatedAt: Long, val contentDigest: List<Byte>)
    private fun version(stored: StoredSession): Version {
        // Timestamp and length can stay equal when content changes. Hash UTF-16 code units
        // in bounded chunks, avoiding another full payload allocation or retained copy.
        val digest = MessageDigest.getInstance("SHA-256")
        val buffer = ByteArray(4096)
        for (text in listOf(stored.state, stored.metadata)) {
            var used = 0
            for (char in text) {
                buffer[used++] = (char.code ushr 8).toByte()
                buffer[used++] = char.code.toByte()
                if (used == buffer.size) { digest.update(buffer); used = 0 }
            }
            digest.update(buffer, 0, used)
            // Include lengths to distinguish the boundary between state and metadata.
            for (shift in listOf(24, 16, 8, 0)) digest.update((text.length ushr shift).toByte())
        }
        return Version(stored.updatedAt, digest.digest().toList())
    }
    private var cache: Map<String, Pair<Version, SessionListSummary>> = emptyMap()
    var decoded = 0
        private set

    fun update(rows: List<StoredSession>): Map<String, SessionListSummary> {
        val next = HashMap<String, Pair<Version, SessionListSummary>>(rows.size)
        for (stored in rows) {
            val key = sessionKey(stored)
            val version = version(stored)
            next[key] = cache[key]?.takeIf { it.first == version } ?: (version to sessionListSummary(stored).also { decoded++ })
        }
        cache = next
        return next.mapValues { it.value.second }
    }
}
