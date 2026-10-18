package dev.acportal.protocol

import kotlinx.serialization.Serializable

/**
 * One entry of the conversation as shown: an item on its own, or the steps of a finished turn
 * folded into one line ("Worked for 1m 12s · 5 steps"). [Steps.id] is the id of the user message
 * that started the turn, which stays the same when the conversation is replayed.
 */
sealed interface TurnBlock {
    val key: String
    data class Row(val item: TimelineItem) : TurnBlock { override val key: String get() = item.id }
    data class Steps(val id: String, val items: List<TimelineItem>, val tools: Int) : TurnBlock { override val key: String get() = "steps:$id" }
}

private fun TimelineItem.isUserMessage() = (this is TimelineItem.Text && role == "user") || (this is TimelineItem.Content && role == "user")
private fun TimelineItem.isAgentMessage() = (this is TimelineItem.Text && role == "agent") || (this is TimelineItem.Content && role == "agent")

/**
 * After a turn has its final reply, everything before that reply (thoughts, tool calls, plans and
 * earlier remarks) folds into one [TurnBlock.Steps]. The running turn, a turn with no final agent
 * reply, a turn that shows an error and a turn that still waits for an answer are never folded.
 * One pass; the items are not copied unless a turn folds.
 */
fun foldTurns(items: List<TimelineItem>, processing: Boolean, waiting: Boolean): List<TurnBlock> {
    val out = ArrayList<TurnBlock>(items.size)
    var i = 0
    while (i < items.size) {
        val first = items[i]
        if (!first.isUserMessage()) { out += TurnBlock.Row(first); i++; continue }
        var end = i + 1
        while (end < items.size && !items[end].isUserMessage()) end++
        out += TurnBlock.Row(first)
        var finalAt = -1
        for (index in end - 1 downTo i + 1) if (items[index].isAgentMessage()) { finalAt = index; break }
        val open = end == items.size && (processing || waiting)
        val before = if (finalAt > i + 1) items.subList(i + 1, finalAt) else emptyList()
        val hasError = before.any { it is TimelineItem.Notice && it.error }
        if (open || finalAt < 0 || before.isEmpty() || hasError) {
            for (index in i + 1 until end) out += TurnBlock.Row(items[index])
        } else {
            out += TurnBlock.Steps(first.id, before, before.count { it is TimelineItem.Tool })
            for (index in finalAt until end) out += TurnBlock.Row(items[index])
        }
        i = end
    }
    return out
}

/** 45s, 3m 05s, 1h 12m. */
fun formatDuration(millis: Long): String {
    val seconds = (millis.coerceAtLeast(0) + 500) / 1000
    if (seconds < 60) return "${seconds}s"
    val minutes = seconds / 60
    if (minutes < 60) return "${minutes}m ${(seconds % 60).toString().padStart(2, '0')}s"
    return "${minutes / 60}h ${(minutes % 60).toString().padStart(2, '0')}m"
}

/** The short form for a narrow list: 45s, 3m, 1h 12m. */
fun briefDuration(millis: Long): String {
    val seconds = (millis.coerceAtLeast(0) + 500) / 1000
    if (seconds < 60) return "${seconds}s"
    val minutes = (seconds + 30) / 60
    return if (minutes < 60) "${minutes}m" else "${minutes / 60}h ${(minutes % 60).toString().padStart(2, '0')}m"
}

/** "5 steps", "1 step", or "thinking" when the turn only thought. */
fun stepsLabel(block: TurnBlock.Steps): String = when (block.tools) { 0 -> "thinking"; 1 -> "1 step"; else -> "${block.tools} steps" }

/** How long each turn took, measured by this phone while it watched the turn run. */
@Serializable
data class TurnTime(val at: Long, val took: Long? = null)
fun workedMillis(times: Map<String, TurnTime>?): Long = times?.values?.sumOf { it.took ?: 0L } ?: 0L

/** Turn times by "host/session" and then by the id of the user message that started the turn. */
typealias TurnTimes = Map<String, Map<String, TurnTime>>

/** Pure updates, so they can be tested without a clock or storage. Safe to apply twice. */
object TurnClock {
    const val MAX_SESSIONS = 64
    const val MAX_TURNS = 200
    fun start(all: TurnTimes, key: String, turnId: String, now: Long): TurnTimes {
        val turns = all[key].orEmpty()
        if (turnId in turns) return all
        val kept = (turns + (turnId to TurnTime(now))).entries.sortedBy { it.value.at }.takeLast(MAX_TURNS).associate { it.key to it.value }
        val next = (all - key) + (key to kept)
        return if (next.size > MAX_SESSIONS) next.entries.sortedBy { e -> e.value.values.maxOfOrNull { it.at } ?: 0L }.takeLast(MAX_SESSIONS).associate { it.key to it.value } else next
    }
    fun end(all: TurnTimes, key: String, turnId: String, now: Long): TurnTimes {
        val turns = all[key] ?: return all
        val time = turns[turnId] ?: return all
        if (time.took != null) return all
        return all + (key to (turns + (turnId to time.copy(took = (now - time.at).coerceAtLeast(0)))))
    }
}
