package dev.acportal.protocol

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.*

val WireJson = Json { ignoreUnknownKeys = true }
fun JsonElement?.objectValue(): JsonObject = this as? JsonObject ?: JsonObject(emptyMap())
fun JsonElement?.arrayValue(): JsonArray = this as? JsonArray ?: JsonArray(emptyList())
fun JsonElement?.text(): String = (this as? JsonPrimitive)?.contentOrNull.orEmpty()
fun JsonElement?.flag(): Boolean = (this as? JsonPrimitive)?.booleanOrNull == true
fun JsonElement?.number(): Long = (this as? JsonPrimitive)?.longOrNull ?: 0

@Serializable
data class AgentInfo(val id: String, val name: String, val enabled: Boolean = true, val installed: Boolean = false, val status: String = "missing",val executable:String?=null,val version:String?=null,val reason:String?=null)
fun AgentInfo.canStartSession():Boolean = enabled && installed && status!="misconfigured"
@Serializable
data class Workspace(val path: String, val name: String)
@Serializable
data class WorkspaceAccess(val readFiles:Boolean=true,val writeFiles:Boolean=true,val terminal:Boolean=true)
@Serializable
data class SessionInfo(
    val id: String, val agentId: String, val acpSessionId: String,
    val workspace: String, val initialization: JsonObject = JsonObject(emptyMap()),
    val setup: JsonObject = JsonObject(emptyMap()), val status: String = "ready",
    val workspaceAccess:WorkspaceAccess=WorkspaceAccess(),
)
@Serializable
data class HostStatus(val hostId: String, val name: String, val version: String, val activeSessions: Int = 0)
@Serializable
data class PairingResult(val hostId: String, val deviceId: String, val token: String, val expiresAt: Long)

@Serializable
sealed interface TimelineItem {
    val id: String
    @Serializable
    data class Text(override val id: String, val role: String, val text: String,val messageId:String?=null,val promptText:String?=null) : TimelineItem
    @Serializable
    data class Content(override val id:String,val role:String,val content:JsonObject) : TimelineItem
    @Serializable
    data class Tool(override val id: String, val title: String, val kind: String = "other", val status: String = "pending", val content: JsonArray = JsonArray(emptyList()), val locations: JsonArray = JsonArray(emptyList())) : TimelineItem
    @Serializable
    data class Plan(override val id: String, val entries: JsonArray) : TimelineItem
    @Serializable
    data class Notice(override val id: String, val text: String, val error: Boolean = false) : TimelineItem
}
@Serializable
data class Permission(val id: JsonElement, val request: JsonObject) {
    val title: String get() = request["toolCall"].objectValue()["title"].text().ifBlank { "Agent operation" }
    val options: JsonArray get() = request["options"].arrayValue()
}
@Serializable
enum class SessionErrorOrigin { AGENT, AUTHENTICATION, CONNECTION, PROTOCOL, SESSION }
@Serializable
data class SessionState(
    val sequence: Long = 0,
    val items: List<TimelineItem> = emptyList(),
    val permissions: Map<String, Permission> = emptyMap(),
    val processing: Boolean = false,
    val replaying: Boolean = false,
    val historyGap: Boolean = false,
    val error: String? = null,
    val modes: JsonObject = JsonObject(emptyMap()),
    val models: JsonObject = JsonObject(emptyMap()),
    val configOptions: JsonArray = JsonArray(emptyList()),
    val commands: JsonArray = JsonArray(emptyList()),
    val terminals: Map<String,JsonObject> = emptyMap(),
    val modelRequests:Map<String,String> = emptyMap(),
    val promptRequestId:String? = null,
    val localHistoryCleared:Boolean = false,
    val errorOrigin:SessionErrorOrigin = SessionErrorOrigin.AGENT,
    val sessionClosed:Boolean = false,
    val sessionInfo:JsonObject = JsonObject(emptyMap()),
    val usage:JsonObject = JsonObject(emptyMap()),
    val replayPermissions:Map<String,Permission>? = null,
)
fun SessionInfo.supportsLoad(): Boolean = initialization["agentCapabilities"].objectValue()["loadSession"].flag()
fun SessionInfo.statusLabel(): String = when(status) {
    "authentication_required" -> "Sign-in required"
    "running" -> "Working"
    "ready" -> "Ready"
    "exited" -> "Stopped"
    "interrupted" -> "Interrupted by host restart"
    else -> status.replace('_',' ').replaceFirstChar(Char::uppercase)
}
fun SessionInfo.authenticationMethods(): List<JsonObject> = initialization["authMethods"].arrayValue().map { it.objectValue() }.filter {
    it["id"].text().isNotBlank() && it["type"].text() in listOf("", "agent")
}
fun SessionInfo.promptCapability(name: String): Boolean = initialization["agentCapabilities"].objectValue()["promptCapabilities"].objectValue()[name].flag()
fun request(id: String, method: String, params: JsonObject): JsonObject = buildJsonObject {
    put("jsonrpc", "2.0"); put("id", id); put("method", method); put("params", params)
}
fun cancel(sessionId: String): JsonObject = buildJsonObject {
    put("jsonrpc", "2.0"); put("method", "session/cancel"); putJsonObject("params") { put("sessionId",sessionId) }
}
fun permissionResponse(permission: Permission, optionId: String): JsonObject {
    require(permission.options.any { it.objectValue()["optionId"].text() == optionId }) { "Permission option is unavailable" }
    return buildJsonObject {
        put("jsonrpc", "2.0"); put("id", permission.id)
        putJsonObject("result") { putJsonObject("outcome") { put("outcome", "selected"); put("optionId", optionId) } }
    }
}
