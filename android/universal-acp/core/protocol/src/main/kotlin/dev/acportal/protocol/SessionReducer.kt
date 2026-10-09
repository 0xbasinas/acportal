package dev.acportal.protocol

import kotlinx.serialization.json.*

class PendingPermissionLimitException : IllegalStateException("Pending permissions exceeded the client retention limit.")

object SessionReducer {
    /** Retained-size limit per agent configuration field (modes, models, config options, commands, usage); three bytes per UTF-16 character bounds UTF-8. */
    const val METADATA_FIELD_BYTES = 2L * 1024 * 1024
    private fun oversized(value: JsonElement) = value.toString().length.toLong() * 3 > METADATA_FIELD_BYTES
    /**
     * Keeps each agent configuration field within [METADATA_FIELD_BYTES]. An oversized new value is
     * dropped and the previous value (or an empty one) is kept, with a visible protocol error.
     */
    fun limitMetadata(previous: SessionState, next: SessionState): SessionState {
        if (next.modes === previous.modes && next.models === previous.models && next.configOptions === previous.configOptions &&
            next.commands === previous.commands && next.usage === previous.usage) return next
        var exceeded = false
        fun limitedObject(value: JsonObject, before: JsonObject): JsonObject =
            if (value === before || !oversized(value)) value else { exceeded = true; if (oversized(before)) JsonObject(emptyMap()) else before }
        fun limitedArray(value: JsonArray, before: JsonArray): JsonArray =
            if (value === before || !oversized(value)) value else { exceeded = true; if (oversized(before)) JsonArray(emptyList()) else before }
        val result = next.copy(modes = limitedObject(next.modes, previous.modes), models = limitedObject(next.models, previous.models),
            configOptions = limitedArray(next.configOptions, previous.configOptions), commands = limitedArray(next.commands, previous.commands),
            usage = limitedObject(next.usage, previous.usage))
        if (!exceeded) return next
        return result.copy(historyGap = true, error = "Agent configuration exceeded the retained limit. Previous choices were kept.",
            errorOrigin = SessionErrorOrigin.PROTOCOL)
    }
    fun reduce(state: SessionState, envelope: JsonObject): SessionState {
        val next = limitMetadata(state, reduceEvent(state, envelope))
        if (next === state) return state
        if (next.permissions !== state.permissions || next.replayPermissions !== state.replayPermissions) {
            var permissionBytes = 0L
            for (permissions in listOfNotNull(next.permissions, next.replayPermissions)) {
                if (permissions.size > 128) throw PendingPermissionLimitException()
                for ((id, permission) in permissions) {
                    permissionBytes += (id.length.toLong() + permission.id.toString().length + permission.request.toString().length) * 3 + 1024
                    if (permissionBytes > 8L * 1024 * 1024) throw PendingPermissionLimitException()
                }
            }
        }
        if (next.terminals === state.terminals && next.sessionInfo === state.sessionInfo) return next
        var omitted = false
        var terminalBytes = 0L
        val terminals = linkedMapOf<String, JsonObject>()
        for ((id, terminal) in next.terminals.entries.toList().asReversed()) {
            val cost = (id.length.toLong() + terminal.toString().length) * 3 + 1024
            if (terminals.size >= 16 || cost > 2L * 1024 * 1024 - terminalBytes) {
                omitted = true
                continue
            }
            terminalBytes += cost
            terminals[id] = terminal
        }
        var result = next.copy(terminals = terminals.entries.toList().asReversed().associate { it.toPair() }, historyGap = next.historyGap || omitted)
        if (next.sessionInfo.toString().length.toLong() * 3 > 2L * 1024 * 1024) {
            val previous = state.sessionInfo.takeIf { it.toString().length.toLong() * 3 <= 2L * 1024 * 1024 } ?: JsonObject(emptyMap())
            result = result.copy(sessionInfo = previous, historyGap = true,
                error = "Session metadata exceeded the retained limit. Previous details were kept.", errorOrigin = SessionErrorOrigin.PROTOCOL)
        }
        return result
    }
    private fun reduceEvent(state: SessionState, envelope: JsonObject): SessionState {
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
            } else if(method=="session/set_model" && message["id"]!=null && params["modelId"].text().length<=1024) {
                next=next.copy(modelRequests=(next.modelRequests+(message["id"].toString() to params["modelId"].text())).entries.toList().takeLast(64).associate {it.toPair()})
            } else if (method.isEmpty() && message["result"] != null) {
                next = next.copy(permissions = next.permissions - message["id"].toString())
            }
            return bounded(next)
        }
        if ((method == PERMISSION_METHOD || method == ELICITATION_METHOD) && message["id"] != null) return permission(next, message)
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
        if(envelope["direction"].text()=="host" && terminalId.isNotBlank())next=next.copy(terminals=(next.terminals-terminalId)+(terminalId to terminal))
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
                    (update["locations"] as? JsonArray) ?: previous?.locations ?: JsonArray(emptyList()),
                    autoReview(envelope,update) ?: previous?.autoReview,
                    hostWrite(envelope,update) ?: previous?.hostWrite)
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
    /** Only the host may report an auto-review; the shell line it decided is kept with it (bounded). */
    private fun autoReview(envelope:JsonObject,update:JsonObject):JsonObject? {
        if(envelope["direction"].text()!="host")return null
        val review=update["_meta"].objectValue()["acpdAutoReview"] as? JsonObject ?: return null
        val line=update["rawInput"].objectValue()["shellLine"].text()
        return buildJsonObject {
            put("decision",review["decision"].text().take(16));put("layer",review["layer"].text().take(16));put("reason",review["reason"].text().take(400))
            if(line.isNotEmpty())put("shellLine",line.take(16000))
        }
    }
    /** Only the host reports host file-write outcomes; unknown values are ignored. */
    private fun hostWrite(envelope:JsonObject,update:JsonObject):String? {
        if(envelope["direction"].text()!="host")return null
        return update["_meta"].objectValue()["acpdWrite"].text().takeIf {hostWriteLabel(it)!=null}
    }
    private fun permission(state: SessionState, message: JsonObject,snapshot:Boolean=false): SessionState {
        val id = message["id"] ?: return state
        val request = message["params"].objectValue()
        val method = if (message["method"].text() == ELICITATION_METHOD) ELICITATION_METHOD else PERMISSION_METHOD
        if (method == ELICITATION_METHOD) { if (request["mode"].text() != "form" || request["requestedSchema"] !is JsonObject) return state }
        else if (request["options"] !is JsonArray) return state
        val permission = Permission(id, request, method)
        if(snapshot && state.replayPermissions!=null)return state.copy(replayPermissions=state.replayPermissions+(id.toString() to permission))
        return state.copy(permissions = state.permissions + (id.toString() to permission))
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
                is TimelineItem.Tool->(item.content.toString().length.toLong()+item.locations.toString().length+item.title.length+(item.autoReview?.toString()?.length ?: 0))*3+1024
                is TimelineItem.Plan->item.entries.toString().length*3L+1024
                is TimelineItem.Notice->item.text.length*3L+1024
            }
            if(bytes+cost>8L*1024*1024) {omitted=true;break}
            bytes+=cost;retained.add(item)
        }
        return state.copy(items=retained.asReversed(),historyGap=state.historyGap || omitted)
    }
}
