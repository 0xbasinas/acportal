package dev.acportal.data

import dev.acportal.protocol.*
import dev.acportal.storage.*
import kotlinx.serialization.json.*

/** Status shown for a saved session whose stored details could not be read. */
const val STORED_DETAILS_UNAVAILABLE = "details_unavailable"

/** UTF-8 length of [text] up to [limit] + 1, without building a byte array. */
internal fun utf8Bytes(text: String, limit: Int = Int.MAX_VALUE - 4): Int {
    var size = 0
    var index = 0
    while (index < text.length && size <= limit) {
        val char = text[index]
        val pair = Character.isHighSurrogate(char) && index + 1 < text.length && Character.isLowSurrogate(text[index + 1])
        size += when { pair -> 4; char.code < 0x80 -> 1; char.code < 0x800 -> 2; else -> 3 }
        index += if (pair) 2 else 1
    }
    return size
}

/**
 * The draft copy to store: at most [STORED_DRAFT_MAX_BYTES] of UTF-8, cut at a character
 * boundary. Only the saved copy is shortened; the limit equals the largest prompt the app
 * sends ([MAX_PROMPT_WIRE_BYTES]), so any draft that could still be sent is kept whole.
 */
internal fun storedDraft(text: String): String {
    if (text.length.toLong() * 3 <= STORED_DRAFT_MAX_BYTES) return text
    var size = 0
    var index = 0
    while (index < text.length) {
        val char = text[index]
        val pair = Character.isHighSurrogate(char) && index + 1 < text.length && Character.isLowSurrogate(text[index + 1])
        val bytes = when { pair -> 4; char.code < 0x80 -> 1; char.code < 0x800 -> 2; else -> 3 }
        if (size + bytes > STORED_DRAFT_MAX_BYTES) break
        size += bytes
        index += if (pair) 2 else 1
    }
    return if (index == text.length) text else text.substring(0, index)
}

/**
 * Session details to store, within [STORED_METADATA_MAX_BYTES]. Agent setup choices (modes,
 * models, configuration) are dropped first: opening a session reads them from the host
 * again. If that is still too large, only the capabilities the offline pages use are kept.
 */
internal fun storedMetadata(info: SessionInfo): String {
    val full = WireJson.encodeToString(info)
    if (utf8Bytes(full, STORED_METADATA_MAX_BYTES) <= STORED_METADATA_MAX_BYTES) return full
    val withoutSetup = WireJson.encodeToString(info.copy(setup = JsonObject(emptyMap())))
    if (utf8Bytes(withoutSetup, STORED_METADATA_MAX_BYTES) <= STORED_METADATA_MAX_BYTES) return withoutSetup
    val initialization = info.initialization
    val essential = buildJsonObject {
        put("agentCapabilities", buildJsonObject {
            put("loadSession", initialization["agentCapabilities"].objectValue()["loadSession"].flag())
        })
        initialization["agentInfo"].objectValue()["name"].text().take(256).takeIf { it.isNotBlank() }?.let { name ->
            put("agentInfo", buildJsonObject { put("name", name) })
        }
    }
    val minimal = WireJson.encodeToString(info.copy(setup = JsonObject(emptyMap()), initialization = essential))
    require(utf8Bytes(minimal, STORED_METADATA_MAX_BYTES) <= STORED_METADATA_MAX_BYTES) { "Session details are too large to save" }
    return minimal
}

/**
 * Saved session details, tolerating rows that older versions wrote or that a guarded query
 * replaced (empty text): those show as "Details unavailable" until the host's copy replaces
 * them, instead of failing the whole Sessions page.
 */
fun storedSessionInfo(stored: StoredSession): SessionInfo =
    runCatching { WireJson.decodeFromString<SessionInfo>(stored.metadata) }.getOrNull()
        ?: SessionInfo(stored.id, agentId = "", acpSessionId = "", workspace = "", status = STORED_DETAILS_UNAVAILABLE)

/** Saved conversation copy; an unreadable copy is shown as empty with a history gap. */
fun storedSessionState(stored: StoredSession): SessionState =
    if (stored.state.isEmpty()) SessionState()
    else runCatching { WireJson.decodeFromString<SessionState>(stored.state) }.getOrElse { SessionState(historyGap = true) }
