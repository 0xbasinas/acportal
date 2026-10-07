package dev.acportal.protocol

private fun messageHeading(role:String,agent:String)=when(role) {"user"->"You";"thought"->"Agent thoughts · ${heading(agent)}";else->heading(agent)}

/** A readable snapshot, excluding drafts, credential configuration, raw envelopes and pending decisions. */
fun transcriptMarkdown(info:SessionInfo,state:SessionState):String {
    val out=MarkdownSnapshot()
    out.add("# Conversation\n\nAgent: ${heading(info.agentId)}\n\nWorkspace:\n")
    out.code(info.workspace)
    out.add("Snapshot of messages available on this device.\n\n")
    if(state.historyGap || state.localHistoryCleared)out.add("Earlier messages are not included in this snapshot.\n\n")
    if(state.processing || state.replaying)out.add("The session was still working or restoring messages when this snapshot was taken.\n\n")
    state.items.forEach {item->when(item) {
        is TimelineItem.Text->{out.add("## ${messageHeading(item.role,info.agentId)}\n\n");out.add(item.text);out.add("\n\n")}
        is TimelineItem.Content->{out.add("## ${messageHeading(item.role,info.agentId)}\n\n");out.content(item.content)}
        is TimelineItem.Notice->{out.add("${if(item.error)"Error" else "Status"}: ${item.text}\n\n")}
        is TimelineItem.Plan->{out.add("## Plan\n\n");item.entries.forEach {entry->val value=entry.objectValue();out.add("- ${heading(value["status"].text())}: ${value["content"].text()}\n")};out.add("\n")}
        is TimelineItem.Tool->{
            out.add("## ${heading(item.title)}\n\n${heading(item.kind)} · ${heading(item.status)}\n\n")
            item.locations.forEach {location->location.objectValue()["path"].text().takeIf {it.isNotBlank()}?.let(out::code)}
            item.content.forEach {content->val block=content.objectValue();when(block["type"].text()) {
                "diff"->{out.code(block["path"].text());out.add("Before:\n\n");out.code(block["oldText"]?.takeUnless {it==kotlinx.serialization.json.JsonNull}?.text() ?: "(new file)");out.add("After:\n\n");out.code(block["newText"].text())}
                "content"->out.content(block["content"].objectValue())
                "terminal"->{
                    val terminal=state.terminals[block["terminalId"].text()]
                    if(terminal==null)out.add("Terminal output is no longer available on this device.\n\n")
                    else {
                        if(terminal["truncated"].flag())out.add("Earlier terminal output was omitted.\n\n")
                        out.code(terminal["output"].text())
                        terminal["exitStatus"]?.objectValue()?.let {exit->
                            exit["exitCode"]?.takeUnless {it==kotlinx.serialization.json.JsonNull}?.let {out.add("Exit code: ${heading(it.text())}\n\n")}
                            exit["signal"]?.takeUnless {it==kotlinx.serialization.json.JsonNull}?.let {out.add("Signal: ${heading(it.text())}\n\n")}
                        }
                    }
                }
                else->out.add("Additional tool content\n\n")
            }}
        }
    }}
    if(state.items.isEmpty())out.add("No messages available on this device.\n")
    return out.toString()
}

private fun heading(value:String):String = value.lineSequence().joinToString(" ").replace(Regex("([\\\\`*_{}\\[\\]<>#])"),"\\\\$1")

private class MarkdownSnapshot {
    private val text=StringBuilder()
    private var bytes=0
    fun add(value:String) {
        require(value.length<=8*1024*1024) {"The conversation exceeds the export size limit"}
        bytes+=value.toByteArray(Charsets.UTF_8).size
        require(bytes<=8*1024*1024) {"The conversation exceeds the export size limit"}
        text.append(value)
    }
    fun code(value:String) {
        require(value.length<=8*1024*1024) {"The conversation exceeds the export size limit"}
        var run=0;var longest=0
        value.forEach {char->if(char=='`') {run++;longest=maxOf(longest,run)} else run=0}
        val fence="`".repeat(maxOf(3,longest+1))
        add("$fence\n");add(value);add("\n$fence\n\n")
    }
    fun content(block:kotlinx.serialization.json.JsonObject) {
        val resource=contentResource(block)
        if(block["type"].text()=="text") {code(block["text"].text());return}
        add("${heading(contentName(block))} · ${heading(block["type"].text())}\n\n")
        resource["uri"].text().takeIf {it.isNotBlank()}?.let(::code)
        if(block["type"].text()=="resource" && resource["text"]!=null)code(resource["text"].text())
        else if(block["type"].text()!="resource_link")add("Embedded media is available in the app and is not included in this Markdown snapshot.\n\n")
    }
    override fun toString():String=text.toString()
}
