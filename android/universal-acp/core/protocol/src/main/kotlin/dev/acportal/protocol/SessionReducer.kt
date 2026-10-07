package dev.acportal.protocol

import kotlinx.serialization.json.*

object SessionReducer {
    fun reduce(state: SessionState, envelope: JsonObject): SessionState {
        when (envelope["type"].text()) {
            "replay_start" -> return state.copy(replaying = true, permissions=emptyMap(),replayPermissions=emptyMap(),historyGap = state.historyGap || envelope["gap"].flag(), processing = envelope["processing"].flag())
            "replay_complete" -> return state.copy(replaying = false,permissions=state.replayPermissions ?: state.permissions,replayPermissions=null,sequence = maxOf(state.sequence, envelope["latestSequence"].number()), error=if(state.errorOrigin==SessionErrorOrigin.CONNECTION)null else state.error)
            "pending_permission" -> return permission(state, envelope["message"].objectValue(),snapshot=true)
            "session_closed" -> return state.copy(processing = false, promptRequestId=null, permissions = emptyMap(),replayPermissions=null, modelRequests=emptyMap(), error = envelope["reason"].text().ifBlank {"The agent session stopped."},errorOrigin=SessionErrorOrigin.SESSION,sessionClosed=true)
            "reconnect_required" -> return state.copy(error = "Connection fell behind. Reconnecting…",errorOrigin=SessionErrorOrigin.CONNECTION)
            "error" -> return state.copy(error = envelope["code"].text().replace('_',' '),errorOrigin=SessionErrorOrigin.PROTOCOL)
            "event" -> {}
            else -> return state
        }
        val sequence = envelope["sequence"].number()
        if (sequence <= state.sequence) return state
        val message = envelope["message"].objectValue()
        var next = state.copy(sequence = sequence)
        val method = message["method"].text()
        val params = message["params"].objectValue()
        if (envelope["direction"].text() == "client") {
            if (method == "session/prompt") {
                val text = promptDisplay(params["prompt"].arrayValue())
                next = next.copy(items = next.items + TimelineItem.Text("user-${message["id"].text()}", "user", text,promptText=promptText(params["prompt"].arrayValue())), processing = true, promptRequestId=message["id"].toString(), error = null)
            } else if(method=="session/set_model" && message["id"]!=null) {
                next=next.copy(modelRequests=(next.modelRequests+(message["id"].toString() to params["modelId"].text())).entries.toList().takeLast(64).associate {it.toPair()})
            } else if (method.isEmpty() && message["result"] != null) {
                next = next.copy(permissions = next.permissions - message["id"].toString())
            }
            return bounded(next)
        }
        if (method == "session/request_permission") return permission(next, message)
        if (method.isEmpty()) {
            val requestedModel=next.modelRequests[message["id"].toString()]
            if(requestedModel!=null) {
                next=next.copy(modelRequests=next.modelRequests-message["id"].toString())
                if(message["result"]!=null && message["error"]==null)next=next.copy(models=JsonObject(next.models+("currentModelId" to JsonPrimitive(requestedModel))))
            }
            if (message["error"] != null) {
                val promptFailed=next.promptRequestId==message["id"].toString()
                return bounded(next.copy(processing=if(promptFailed)false else next.processing,promptRequestId=if(promptFailed)null else next.promptRequestId,error=message["error"].objectValue()["message"].text().ifBlank {"The agent could not complete this request."},errorOrigin=SessionErrorOrigin.AGENT))
            }
            if (message["result"].objectValue()["stopReason"] != null) return next.copy(processing = false, promptRequestId=null, permissions = emptyMap())
            val configuration=message["result"].objectValue()["configOptions"]
            if(configuration is JsonArray)return next.copy(configOptions=configuration)
            return next
        }
        if (method != "session/update") return next
        val update = params["update"].objectValue()
        val terminal=update["_meta"].objectValue()["acpdTerminal"].objectValue()
        val terminalId=terminal["terminalId"].text()
        if(envelope["direction"].text()=="host" && terminalId.isNotBlank())next=next.copy(terminals=((next.terminals-terminalId)+(terminalId to terminal)).entries.toList().takeLast(16).associate {it.toPair()})
        when (val kind = update["sessionUpdate"].text()) {
            "agent_message_chunk", "user_message_chunk", "agent_thought_chunk" -> {
                val role = when(kind) {"agent_thought_chunk"->"thought";"agent_message_chunk"->"agent";else->"user"}
                val content = update["content"].objectValue()
                if (content["type"].text() != "text") return bounded(next.copy(items=next.items+TimelineItem.Content("content-$sequence",role,content)))
                val text = content["text"].text()
                val previous = next.items.lastOrNull() as? TimelineItem.Text
                val messageId=update["messageId"].text().takeIf {it.isNotBlank()}
                val items = if (previous?.role == role && previous.messageId==messageId) next.items.dropLast(1) + previous.copy(text = previous.text + text)
                    else next.items + TimelineItem.Text("text-$sequence", role, text,messageId)
                next = next.copy(items = items)
            }
            "tool_call", "tool_call_update" -> {
                val id = update["toolCallId"].text()
                val previous = next.items.filterIsInstance<TimelineItem.Tool>().find { it.id == id }
                val item = TimelineItem.Tool(id,
                    update["title"].text().ifBlank { previous?.title ?: "Tool activity" },
                    update["kind"].text().ifBlank { previous?.kind ?: "other" },
                    update["status"].text().ifBlank { previous?.status ?: "pending" },
                    (update["content"] as? JsonArray) ?: previous?.content ?: JsonArray(emptyList()),
                    (update["locations"] as? JsonArray) ?: previous?.locations ?: JsonArray(emptyList()))
                next = next.copy(items = if (previous == null) next.items + item else next.items.map { if (it.id == id) item else it })
            }
            "plan" -> next = next.copy(items = next.items.filterNot { it is TimelineItem.Plan } + TimelineItem.Plan("plan", update["entries"].arrayValue()))
            "available_commands_update" -> next = next.copy(commands = update["availableCommands"].arrayValue())
            "current_mode_update" -> next = next.copy(modes = JsonObject(next.modes + ("currentModeId" to (update["currentModeId"] ?: JsonNull))))
            "config_option_update" -> next = next.copy(configOptions = update["configOptions"].arrayValue())
            "session_info_update" -> next=next.copy(sessionInfo=JsonObject(next.sessionInfo+update.filterKeys {it!="sessionUpdate"}))
            "usage_update" -> next=next.copy(usage=JsonObject(update.filterKeys {it!="sessionUpdate"}))
            else -> next = next.copy(items = next.items + TimelineItem.Notice("unknown-$sequence", kind.replace('_', ' ').replaceFirstChar(Char::uppercase)))
        }
        return bounded(next)
    }
    private fun permission(state: SessionState, message: JsonObject,snapshot:Boolean=false): SessionState {
        val id = message["id"] ?: return state
        val request = message["params"].objectValue()
        if (request["options"] !is JsonArray) return state
        if(snapshot && state.replayPermissions!=null)return state.copy(replayPermissions=state.replayPermissions+(id.toString() to Permission(id,request)))
        return state.copy(permissions = state.permissions + (id.toString() to Permission(id, request)))
    }
    private fun bounded(state:SessionState):SessionState {
        var omitted=false
        val retained=ArrayList<TimelineItem>();var bytes=0L
        for(original in state.items.asReversed()) {
            if(retained.size>=2000) {omitted=true;break}
            val item=if(original is TimelineItem.Text && original.text.length>1024*1024) {omitted=true;original.copy(text=original.text.takeLast(1024*1024))} else original
            // Three bytes per UTF-16 character bounds UTF-8, including non-ASCII text.
            val cost=when(item) {
                is TimelineItem.Text->(item.text.length.toLong()+(item.promptText?.length ?: 0))*3L+item.id.length*3L+1024
                is TimelineItem.Content->item.content.toString().length*3L+1024
                is TimelineItem.Tool->(item.content.toString().length.toLong()+item.locations.toString().length+item.title.length)*3+1024
                is TimelineItem.Plan->item.entries.toString().length*3L+1024
                is TimelineItem.Notice->item.text.length*3L+1024
            }
            if(bytes+cost>8L*1024*1024) {omitted=true;break}
            bytes+=cost;retained.add(item)
        }
        return state.copy(items=retained.asReversed(),historyGap=state.historyGap || omitted)
    }
}
