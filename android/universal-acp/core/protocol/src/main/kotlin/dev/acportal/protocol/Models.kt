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
data class AgentInfo(val id: String, val name: String, val enabled: Boolean = true, val installed: Boolean = false, val status: String = "missing",val executable:String?=null,val version:String?=null,val reason:String?=null,val ownTools:Boolean=false)
fun AgentInfo.canStartSession():Boolean = enabled && installed && status!="misconfigured"
/** Short label for agents the host registry marks with `ownTools`. */
const val OWN_TOOLS_LABEL = "Uses its own tools"
/**
 * Shown for agents the host registry marks with `ownTools`. The host cannot see those
 * actions, so the wording only claims what the host enforces.
 */
const val OWN_TOOLS_WARNING = "This agent can edit files and run commands with its own built-in tools. Those actions do not go through this host's approval prompts or workspace access settings. Only the agent's own permission settings apply to them."
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
    /** The host registry marks this agent with `ownTools` (see [OWN_TOOLS_WARNING]). */
    val ownTools:Boolean=false,
)
@Serializable
data class HostStatus(val hostId: String, val name: String, val version: String, val activeSessions: Int = 0, val features: List<String> = emptyList())
/** Host feature: forwards agent form elicitations when the phone opts in at session creation. */
const val HOST_FEATURE_FORM_ELICITATION = "formElicitation"
@Serializable
data class PairingResult(val hostId: String, val deviceId: String, val token: String, val expiresAt: Long)

/** Counts JSON serialisations made to size timeline items for retention (tests only). */
internal val timelineSizeComputations=java.util.concurrent.atomic.AtomicLong()
private fun serialisedChars(value:JsonElement):Long {timelineSizeComputations.incrementAndGet();return value.toString().length.toLong()}
@Serializable
sealed interface TimelineItem {
    val id: String
    @Serializable
    data class Text(override val id: String, val role: String, val text: String,val messageId:String?=null,val promptText:String?=null) : TimelineItem
    @Serializable
    data class Content(override val id:String,val role:String,val content:JsonObject) : TimelineItem {
        /** Serialised length, computed once per instance (not serialised). */
        internal val contentChars:Long by lazy(LazyThreadSafetyMode.PUBLICATION) {serialisedChars(content)}
    }
    @Serializable
    data class Tool(override val id: String, val title: String, val kind: String = "other", val status: String = "pending", val content: JsonArray = JsonArray(emptyList()), val locations: JsonArray = JsonArray(emptyList()),val autoReview:JsonObject?=null,val hostWrite:String?=null) : TimelineItem {
        internal val contentChars:Long by lazy(LazyThreadSafetyMode.PUBLICATION) {serialisedChars(content)+serialisedChars(locations)+(autoReview?.let(::serialisedChars) ?: 0)}
    }
    @Serializable
    data class Plan(override val id: String, val entries: JsonArray) : TimelineItem {
        internal val contentChars:Long by lazy(LazyThreadSafetyMode.PUBLICATION) {serialisedChars(entries)}
    }
    @Serializable
    data class Notice(override val id: String, val text: String, val error: Boolean = false) : TimelineItem
}
@Serializable
data class Permission(val id: JsonElement, val request: JsonObject, val method: String = PERMISSION_METHOD) {
    /** A form elicitation: the agent asks the user to fill in fields instead of choosing an option. */
    val isElicitation: Boolean get() = method == ELICITATION_METHOD
    val title: String get() = if (isElicitation) request["message"].text().ifBlank { "The agent needs more information" }
        else request["toolCall"].objectValue()["title"].text().ifBlank { "Agent operation" }
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

const val PERMISSION_METHOD = "session/request_permission"
const val ELICITATION_METHOD = "elicitation/create"

/** Host file-write outcomes reported in a tool's `_meta.acpdWrite`. */
const val HOST_WRITE_CHANGED = "changed-without-consent"
const val HOST_WRITE_UNCHANGED = "unchanged"
/** Short badge for a host file-write outcome, or null when the value is unknown. */
fun hostWriteLabel(value:String):String? = when(value) {
    HOST_WRITE_CHANGED->"File changed without your approval"
    HOST_WRITE_UNCHANGED->"Already up to date · nothing written"
    else->null
}
/** Plain explanation shown with the badge. */
fun hostWriteExplanation(value:String):String? = when(value) {
    HOST_WRITE_CHANGED->"You did not approve this write, but the file already holds the new text. The agent probably wrote it with its own tools. Check the change and undo it on the host if needed."
    HOST_WRITE_UNCHANGED->"The agent asked to write text the file already holds, so the host changed nothing and did not ask you."
    else->null
}

/** Host shell-line auto-review shown on a tool card, e.g. "Auto-allowed by rules". */
fun autoReviewLabel(review:JsonObject):String {
    val layer=review["layer"].text().ifBlank {"host"}
    return when(review["decision"].text()) {
        "allow"->"Auto-allowed by $layer"
        "deny"->"Auto-denied by $layer"
        "ask"->"Sent to you by $layer review"
        "reviewing"->"Being reviewed by $layer"
        else->"Auto-review"
    }
}
