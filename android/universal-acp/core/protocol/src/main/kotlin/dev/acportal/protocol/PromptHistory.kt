package dev.acportal.protocol

import kotlinx.serialization.json.JsonArray

/** Only submitted text blocks qualify. Rendered attachment labels and agent echoes do not. */
fun promptText(content:JsonArray):String = content.filter {it.objectValue()["type"].text()=="text"}
    .joinToString("\n") {it.objectValue()["text"].text()}

data class PromptHistoryEntry(val id:String,val text:String)
fun promptHistory(state:SessionState):List<PromptHistoryEntry> = state.items.asReversed().asSequence()
    .filterIsInstance<TimelineItem.Text>()
    .filter {it.role=="user" && !it.promptText.isNullOrBlank()}
    .distinctBy {it.promptText}
    .take(20)
    .map {PromptHistoryEntry(it.id,it.promptText!!)}
    .toList()

/** This session's prompts first, then the folder's prompts shared through the host (from the desktop or another phone). */
fun withSharedHistory(entries:List<PromptHistoryEntry>,shared:List<String>):List<PromptHistoryEntry> {
    val seen=entries.mapTo(HashSet()) {it.text}
    return entries+shared.asSequence().filter {it.isNotBlank() && seen.add(it)}.take(50).mapIndexed {index,text->PromptHistoryEntry("shared-$index",text)}.toList()
}
