package dev.acportal.presentation

import dev.acportal.protocol.TimelineItem

/*
 * Small values derived from the conversation on every streamed update. They are kept
 * here, outside composables, so their cost is testable on the JVM: each runs in O(1)
 * for a streaming tail and allocates no copies of the timeline.
 */

/** First line of the first user message, scanned without copying the timeline. */
internal fun firstUserLine(items:List<TimelineItem>,limit:Int=36):String? {
    for(item in items) if(item is TimelineItem.Text && item.role=="user") return item.text.lineSequence().firstOrNull()?.take(limit)
    return null
}

/**
 * Identity of the timeline tail for follow-latest scrolling: the last item and how much
 * of it has arrived. Comparing this key is constant time, unlike comparing the last
 * item itself, whose equality check walks the whole streamed text on every frame.
 */
internal data class TimelineTailKey(val size:Int,val lastId:String?,val lastLength:Int)
internal fun timelineTailKey(items:List<TimelineItem>):TimelineTailKey {
    val last=items.lastOrNull()
    val length=when(last) {is TimelineItem.Text->last.text.length;is TimelineItem.Tool->last.content.size;else->0}
    return TimelineTailKey(items.size,last?.id,length)
}

/** The last user message, searched from the end: during streaming it is near the tail. */
internal fun lastUserText(items:List<TimelineItem>):String? {
    for(index in items.indices.reversed()) {val item=items[index];if(item is TimelineItem.Text && item.role=="user")return item.text}
    return null
}

/**
 * Which parsed Markdown document to show while a newer source is parsed. A finished
 * parse of the exact source always wins. While text is being appended (the new source
 * extends the parsed one), the previous rendering stays on screen instead of falling
 * back to plain text and back again on every chunk, which re-laid out the whole
 * message twice per update. Any other change shows plain text until its parse is done.
 */
internal fun <T> markdownToShow(parsed:Pair<String,T?>?,source:String):T? {
    parsed ?: return null
    if(parsed.first==source)return parsed.second
    return parsed.second?.takeIf {source.length>parsed.first.length && source.startsWith(parsed.first)}
}
