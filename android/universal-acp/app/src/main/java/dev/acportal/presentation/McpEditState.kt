package dev.acportal.presentation

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import dev.acportal.data.McpDefinition

/** Retained by the page ViewModel only. Never serialize drafts containing secrets. */
class McpEditState {
    var draft by mutableStateOf<McpDraft?>(null)
    fun open(server:McpDefinition) {draft=McpDraft(server)}
    fun close() {draft=null}
}

class McpDraft(val server:McpDefinition) {
    var name by mutableStateOf(server.name)
    var transport by mutableStateOf(server.transport)
    var endpoint by mutableStateOf(server.endpoint)
    var arguments by mutableStateOf(server.args.joinToString("\n"))
    var values by mutableStateOf(server.values.joinToString("\n") {"${it.name}=${it.value}"})
    var enabled by mutableStateOf(server.enabled)
    var error by mutableStateOf<String?>(null)
}
