package dev.acportal.data

import dev.acportal.BuildConfig
import dev.acportal.protocol.*
import dev.acportal.security.CredentialVault
import dev.acportal.storage.HostProfile
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.serialization.json.*
import okhttp3.*
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.RequestBody.Companion.toRequestBody
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

fun validateAddress(address: String,debug: Boolean = BuildConfig.DEBUG): HttpUrl {
    val url = address.trim().toHttpUrl()
    require(url.username.isEmpty() && url.password.isEmpty() && url.query == null && url.fragment == null && url.encodedPath == "/") { "Enter a host address without a path or credentials" }
    require(url.isHttps || (debug && url.host in listOf("localhost","127.0.0.1","::1","10.0.2.2"))) { "Use HTTPS for remote hosts" }
    return url
}
class HostApi(val client: OkHttpClient, private val vault: CredentialVault) {
    companion object {
        fun client(): OkHttpClient = OkHttpClient.Builder().connectTimeout(10,TimeUnit.SECONDS).readTimeout(30,TimeUnit.SECONDS).callTimeout(180,TimeUnit.SECONDS).pingInterval(15,TimeUnit.SECONDS).followRedirects(false).followSslRedirects(false).build()
    }
    private suspend fun execute(address: String,path:String,token:String?=null,body:JsonObject?=null,method:String="GET"): JsonElement {
        val base = validateAddress(address)
        val target = base.resolve(path) ?: error("Invalid host endpoint")
        val builder = Request.Builder().url(target).header("Accept","application/json")
        if (token != null) builder.header("Authorization","Bearer $token")
        builder.method(method,body?.toString()?.toRequestBody("application/json".toMediaType()))
        return client.newCall(builder.build()).awaitJson()
    }
    suspend fun pair(address:String,code:String,name:String): PairingResult = WireJson.decodeFromJsonElement(execute(address,"/v1/pair",body=buildJsonObject { put("code",code);put("deviceName",name) },method="POST"))
    suspend fun status(host:HostProfile): HostStatus = WireJson.decodeFromJsonElement(execute(host.address,"/v1/status",vault.read(host.credentialAlias)))
    suspend fun agents(host:HostProfile): List<AgentInfo> = WireJson.decodeFromJsonElement(execute(host.address,"/v1/agents",vault.read(host.credentialAlias)))
    suspend fun workspaces(host:HostProfile): List<Workspace> = WireJson.decodeFromJsonElement(execute(host.address,"/v1/workspaces",vault.read(host.credentialAlias)))
    suspend fun browse(host:HostProfile,path:String): List<Workspace> {
        val url = validateAddress(host.address).newBuilder().encodedPath("/v1/workspaces/browse").addQueryParameter("path",path).build()
        val request = Request.Builder().url(url).header("Authorization","Bearer ${vault.read(host.credentialAlias)}").build()
        return WireJson.decodeFromJsonElement(client.newCall(request).awaitJson())
    }
    suspend fun directories(host:HostProfile,path:String?): DirectoryListing {
        val url = validateAddress(host.address).newBuilder().encodedPath("/v1/filesystem/directories").apply {if(path!=null)addQueryParameter("path",path)}.build()
        val request = Request.Builder().url(url).header("Authorization","Bearer ${vault.read(host.credentialAlias)}").build()
        return WireJson.decodeFromJsonElement(client.newCall(request).awaitJson())
    }
    suspend fun addWorkspace(host:HostProfile,path:String): Workspace = WireJson.decodeFromJsonElement(execute(host.address,"/v1/workspaces",vault.read(host.credentialAlias),buildJsonObject {put("path",path)},"POST"))
    suspend fun removeWorkspace(host:HostProfile,path:String) { execute(host.address,"/v1/workspaces",vault.read(host.credentialAlias),buildJsonObject {put("path",path)},"DELETE") }
    suspend fun discoverAgents(host:HostProfile): List<KnownAgent> = WireJson.decodeFromJsonElement(execute(host.address,"/v1/agents/discover",vault.read(host.credentialAlias)))
    suspend fun checkAgent(host:HostProfile,command:String,env:Map<String,String>): AgentCheck = WireJson.decodeFromJsonElement(execute(host.address,"/v1/agents/check",vault.read(host.credentialAlias),buildJsonObject {put("command",command);if(env.isNotEmpty())put("env",buildJsonObject {env.forEach {(key,value)->put(key,value)}})},"POST"))
    suspend fun addAgent(host:HostProfile,registration:AgentRegistration) { execute(host.address,"/v1/agents",vault.read(host.credentialAlias),buildJsonObject {
        put("id",registration.id);put("name",registration.name);put("command",registration.command)
        put("args",JsonArray(registration.args.map {JsonPrimitive(it)}))
        put("env",buildJsonObject {registration.env.forEach {(key,value)->put(key,value)}})
        if(registration.ownTools)put("ownTools",true)
    },"POST") }
    suspend fun removeAgent(host:HostProfile,id:String) { execute(host.address,"/v1/agents/${java.net.URLEncoder.encode(id,"UTF-8")}",vault.read(host.credentialAlias),method="DELETE") }
    suspend fun sessions(host:HostProfile): List<SessionInfo> = WireJson.decodeFromJsonElement(execute(host.address,"/v1/sessions",vault.read(host.credentialAlias)))
    suspend fun session(host:HostProfile,id:String): SessionInfo = WireJson.decodeFromJsonElement(execute(host.address,"/v1/sessions/$id",vault.read(host.credentialAlias)))
    suspend fun create(host:HostProfile,agentId:String,workspace:String,loadId:String?=null,mcpServers:JsonArray=JsonArray(emptyList()),access:WorkspaceAccess=WorkspaceAccess(),formElicitation:Boolean=false,manualApproval:Boolean=false,booleanOptions:Boolean=false,yolo:Boolean=false): SessionInfo = WireJson.decodeFromJsonElement(execute(host.address,"/v1/sessions",vault.read(host.credentialAlias),buildJsonObject {
        put("agentId",agentId);put("workspace",workspace);loadId?.let { put("loadSessionId",it) }
        put("mcpServers",mcpServers)
        put("workspaceAccess",buildJsonObject {put("readFiles",access.readFiles);put("writeFiles",access.writeFiles);put("terminal",access.terminal);if(formElicitation)put("formElicitation",true);if(manualApproval)put("manualApproval",true);if(booleanOptions)put("booleanOptions",true);if(yolo)put("yolo",true)})
    },"POST"))
    /** What the agent remembers for one approved folder; read-only, nothing is loaded. */
    suspend fun earlierSessions(host:HostProfile,agentId:String,workspace:String): EarlierSessions {
        val url = validateAddress(host.address).newBuilder().encodedPath("/v1/agents/${java.net.URLEncoder.encode(agentId,"UTF-8")}/sessions").addQueryParameter("workspace",workspace).build()
        val request = Request.Builder().url(url).header("Authorization","Bearer ${vault.read(host.credentialAlias)}").build()
        return WireJson.decodeFromJsonElement(client.newCall(request).awaitJson())
    }
    /** The permission choices the host offers for one agent. Read-only; nothing is changed or started. */
    suspend fun presets(host:HostProfile,agentId:String): List<PermissionPreset> {
        val url = validateAddress(host.address).newBuilder().encodedPath("/v1/presets").addQueryParameter("agent",agentId).build()
        val request = Request.Builder().url(url).header("Authorization","Bearer ${vault.read(host.credentialAlias)}").build()
        return WireJson.decodeFromJsonElement<JsonObject>(client.newCall(request).awaitJson())["presets"].arrayValue().map {WireJson.decodeFromJsonElement<PermissionPreset>(it)}
    }
    /**
     * One of the host's fixed git operations in an approved folder. A failing git command is still a reply
     * with its exit code; `diff-new` exits 1 when the file has content, which is the normal case.
     */
    suspend fun git(host:HostProfile,path:String,op:String,params:Map<String,String> = emptyMap(),input:String?=null): Result<String> {
        require(Regex("[a-z-]{1,32}").matches(op)) {"Invalid git operation"}
        val reply=execute(host.address,"/v1/git/$op",vault.read(host.credentialAlias),buildJsonObject {put("path",path);if(params.isNotEmpty())put("params",buildJsonObject {params.forEach {(key,value)->put(key,value)}});if(input!=null)put("input",input)},"POST").objectValue()
        val code=(reply["code"] as? JsonPrimitive)?.intOrNull ?: 1
        val out=reply["out"].text()
        return if(code==0 || (op=="diff-new" && code==1 && out.isNotEmpty())) Result.success(out)
        else Result.failure(IllegalStateException(reply["err"].text().ifBlank {out}.trim().lines().takeLast(3).joinToString(" ").ifBlank {"git exited $code"}))
    }
    /** Opens a shell on the host in an approved folder (feature `terminal`); returns the new terminal's id. */
    suspend fun openTerminal(host:HostProfile,path:String,rows:Int,cols:Int): String =
        execute(host.address,"/v1/terminals",vault.read(host.credentialAlias),buildJsonObject {put("folder",path);put("rows",rows);put("cols",cols)},"POST").objectValue()["id"].text()
    suspend fun closeTerminal(host:HostProfile,id:String) { execute(host.address,"/v1/terminals/$id",vault.read(host.credentialAlias),method="DELETE") }
    /** An agent on the host writes a commit message (`kind` null) or pull request text (`kind` "pullRequest"); read-only. */
    suspend fun commitMessage(host:HostProfile,path:String,agentId:String,kind:String?=null,base:String?=null): JsonObject =
        execute(host.address,"/v1/commit-message",vault.read(host.credentialAlias),buildJsonObject {put("path",path);put("agentId",agentId);if(kind!=null)put("kind",kind);if(base!=null)put("base",base)},"POST").objectValue()
    /** The checked-out branch of a folder, or null when it is detached or not a repository. */
    suspend fun currentBranch(host:HostProfile,path:String): String? {
        val reply=execute(host.address,"/v1/git/rev-parse",vault.read(host.credentialAlias),buildJsonObject {put("path",path)},"POST").objectValue()
        return if(reply["code"].let {(it as? JsonPrimitive)?.intOrNull}==0) branchFromRevParse(reply["out"].text()) else null
    }
    /** Opens the pull request with the host's `gh`; returns its address. */
    suspend fun openPullRequest(host:HostProfile,request:PullRequestRequest): String {
        val reply=execute(host.address,"/v1/pull-requests",vault.read(host.credentialAlias),buildJsonObject {
            put("path",request.path);put("base",request.base);put("head",request.head());put("title",request.title);put("body",request.body);put("draft",request.draft)
        },"POST").objectValue()
        return reply["url"].text()
    }
    suspend fun delete(host:HostProfile,id:String) { execute(host.address,"/v1/sessions/$id",vault.read(host.credentialAlias),method="DELETE") }
    suspend fun revoke(host:HostProfile) { execute(host.address,"/v1/device",vault.read(host.credentialAlias),method="DELETE") }
    suspend fun tasks(host:HostProfile): List<TaskSummary> = WireJson.decodeFromJsonElement(execute(host.address,"/v1/tasks",vault.read(host.credentialAlias)))
    suspend fun task(host:HostProfile,id:String): TaskDetail = WireJson.decodeFromJsonElement(execute(host.address,"/v1/tasks/${taskPath(id)}",vault.read(host.credentialAlias)))
    suspend fun cancelTask(host:HostProfile,id:String) { execute(host.address,"/v1/tasks/${taskPath(id)}/cancel",vault.read(host.credentialAlias),JsonObject(emptyMap()),"POST") }
    suspend fun discardTask(host:HostProfile,id:String): TaskDiscardResult = WireJson.decodeFromJsonElement(execute(host.address,"/v1/tasks/${taskPath(id)}/discard",vault.read(host.credentialAlias),JsonObject(emptyMap()),"POST"))
    suspend fun mergeTask(host:HostProfile,id:String): TaskMergeResult = WireJson.decodeFromJsonElement(execute(host.address,"/v1/tasks/${taskPath(id)}/merge",vault.read(host.credentialAlias),JsonObject(emptyMap()),"POST"))
    suspend fun setTaskSupervision(host:HostProfile,id:String,paused:Boolean) { execute(host.address,"/v1/tasks/${taskPath(id)}/supervisor/${if(paused)"pause" else "resume"}",vault.read(host.credentialAlias),JsonObject(emptyMap()),"POST") }
    /** Drafts, titles and folder prompt history shared with the desktop (host feature `sharedDrafts`). */
    suspend fun shared(host:HostProfile): JsonObject = execute(host.address,"/v1/shared",vault.read(host.credentialAlias)).jsonObject
    suspend fun putDraft(host:HostProfile,id:String,text:String) { execute(host.address,"/v1/shared/drafts/$id",vault.read(host.credentialAlias),buildJsonObject {put("text",text)},"PUT") }
    suspend fun putTitle(host:HostProfile,id:String,title:String) { execute(host.address,"/v1/shared/titles/$id",vault.read(host.credentialAlias),buildJsonObject {put("title",title)},"PUT") }
    suspend fun addHistory(host:HostProfile,workspace:String,text:String) { execute(host.address,"/v1/shared/history",vault.read(host.credentialAlias),buildJsonObject {put("workspace",workspace);put("text",text)},"POST") }
    fun token(host:HostProfile): String = vault.read(host.credentialAlias)
}
private suspend fun Call.awaitJson(): JsonElement = suspendCancellableCoroutine { continuation ->
    continuation.invokeOnCancellation { cancel() }
    enqueue(object : Callback {
        override fun onFailure(call:Call,e:IOException) { if (continuation.isActive) continuation.resumeWithException(IOException("Host could not be reached. Check the address and connection.",e)) }
        override fun onResponse(call:Call,response:Response) {
            response.use {
                try {
                    if (!response.isSuccessful) {
                        // Up to 8 KiB, so a merge conflict naming many files still parses and keeps its code.
                        val failure=runCatching {WireJson.parseToJsonElement(response.peekBody(8192).string()).objectValue()["error"].objectValue()}.getOrNull()
                        val code=failure?.let {runCatching {it["code"].text()}.getOrDefault("")} ?: ""
                        val files=failure?.get("files")?.let {runCatching {it.jsonArray.map {file->file.text()}}.getOrDefault(emptyList())} ?: emptyList()
                        val branch=failure?.get("branch")?.let {runCatching {it.text()}.getOrDefault("")} ?: ""
                        val repository=failure?.get("repository")?.let {runCatching {it.text()}.getOrDefault("")} ?: ""
                        val existing=failure?.get("url")?.let {runCatching {it.text()}.getOrDefault("")} ?: ""
                        error(hostRequestError(response.code,code,response.request.method,response.request.url.encodedPath,files,branch,repository,existing))
                    }
                    val source = (response.body ?: error("Host returned an empty response")).source()
                    if (source.request(8L*1024*1024+1)) error("Host response exceeds size limit")
                    val text = source.readUtf8()
                    val value = if (text.isBlank()) JsonNull else WireJson.parseToJsonElement(text)
                    if (continuation.isActive) continuation.resume(value)
                } catch (failure:Exception) { if (continuation.isActive) continuation.resumeWithException(failure) }
            }
        }
    })
}

internal fun hostRequestError(status:Int,code:String,method:String,path:String,files:List<String> = emptyList(),branch:String="",repository:String="",existingUrl:String=""):String = when {
    path.endsWith("/merge") && code=="merge_conflict" && (files.isNotEmpty() || branch.isNotEmpty()) ->
        "The task's branch conflicts with the parent's folder"+(if(files.isNotEmpty()) " in ${conflictFiles(files)}" else "")+". Nothing was changed; merge the branch by hand on the host."+mergeByHandCommand(branch,repository)
    path.endsWith("/discard") && code=="already_discarded" -> "This task's work was already thrown away."
    path.endsWith("/discard") && code=="nothing_to_discard" -> "There is nothing left to discard: the task changed nothing, or its work was merged."
    path.endsWith("/discard") && (code=="task_not_finished" || code=="worktree_busy") -> "The task is not finished with its worktree yet. Try again shortly."
    path.endsWith("/discard") && code=="git_failed" -> "The host could not remove the work (a folder may still be in use). Try again shortly."
    path.endsWith("/merge") && code=="parent_dirty" -> "The parent's folder has uncommitted changes. Commit or stash them on the host, then merge again."
    path.endsWith("/merge") && code=="merge_conflict" -> "The task's branch conflicts with the parent's folder. Nothing was changed; merge the branch by hand on the host."
    path.endsWith("/merge") && code=="nothing_to_merge" -> "The task's branch has nothing the parent's folder lacks."
    path.endsWith("/merge") && code=="worktree_dirty" -> "The task's worktree still has uncommitted work. Commit it there first."
    path.endsWith("/merge") && code=="already_discarded" -> "This task's work was thrown away, so there is nothing to merge."
    path.endsWith("/merge") && code=="already_merged" -> "This task's branch was already merged."
    path.endsWith("/merge") && (code=="task_not_finished" || code=="worktree_busy") -> "The task is not finished with its worktree yet. Try again shortly."
    path.endsWith("/pull-requests") && code=="already_exists" -> "A pull request for this branch already exists."+(if(existingUrl.isNotBlank()) " "+existingUrl else "")
    path.endsWith("/pull-requests") && code=="gh_not_signed_in" -> "The GitHub CLI (gh) on the host is not signed in. Run gh auth login there."
    path.endsWith("/pull-requests") && code=="gh_unavailable" -> "The GitHub CLI (gh) was not found on the host."
    path.endsWith("/pull-requests") && code=="no_remote" -> "This repository has no GitHub remote gh can use."
    path.endsWith("/pull-requests") && code=="gh_timeout" -> "gh did not finish in time. Check the host and try again."
    path.endsWith("/pull-requests") && code=="invalid_branch_name" -> "That branch or fork owner is not valid."
    path.endsWith("/pull-requests") && code=="body_too_large" -> "The description is too long."
    path.endsWith("/pull-requests") && code=="path_not_allowed" -> "This folder is not allowed on the host."
    path.endsWith("/pull-requests") -> "gh could not open the pull request."
    path.endsWith("/commit-message") && code=="nothing_staged" -> "Nothing is staged."
    path.endsWith("/commit-message") && code=="nothing_to_describe" -> "Nothing to describe since that base."
    path.endsWith("/commit-message") && code=="base_not_found" -> "That base branch was not found."
    path.endsWith("/commit-message") && code=="agent_unavailable" -> "That agent could not start in this folder."
    path.endsWith("/commit-message") && code=="agent_timeout" -> "The agent did not answer in time."
    path.endsWith("/commit-message") && code=="agent_failed" -> "The agent reported an error."
    path.endsWith("/commit-message") && code=="empty_reply" -> "The agent replied with nothing."
    path.endsWith("/commit-message") && code=="path_not_allowed" -> "This folder is not allowed on the host."
    path.endsWith("/commit-message") && code=="not_a_repository" -> "This folder is not a git repository."
    path=="/v1/terminals" && code=="terminal_disabled" -> "Terminals are off on this host. Turn them on in its config.toml ([terminal] enabled = true) and restart acpd."
    path=="/v1/terminals" && code=="too_many_terminals" -> "The host allows only a few terminals at once. End one first."
    path=="/v1/terminals" && code=="path_not_allowed" -> "The host does not allow a terminal in this folder."
    path.startsWith("/v1/git/") && code=="path_not_allowed" -> "The host does not allow that location. Use a folder inside one the host allows."
    path.startsWith("/v1/git/") && code=="unknown_git_operation" -> "This host is older than the app and cannot do that yet. Update and restart acpd."
    path.startsWith("/v1/git/") && code=="git_unavailable" -> "git was not found on the host."
    path.startsWith("/v1/git/") && code=="git_timeout" -> "git did not finish in time."
    code=="unsupported_mcp_transport" -> "This agent does not support an enabled MCP server's transport. Disable that server or choose another transport."
    status==401 && method=="POST" && path=="/v1/pair" -> "Pairing code was rejected. It may have expired or already been used. Run acpd pair on the host for a new code."
    status==401 -> "Credentials expired or pairing failed. Pair this host again."
    code=="task_not_open" -> "This task has already finished."
    code=="not_a_root_task" -> "Supervision can be paused only for a whole tree, from its root task."
    code=="task_not_found" -> "This task is no longer known to the host. Refresh the task list."
    status==409 -> "This session already has a controller."
    status==400 && method=="POST" && path=="/v1/sessions" -> "The host could not start this session. Choose an existing allowed folder and check the selected agent and MCP settings, then press Start session again."
    else -> "Host request failed ($status)."
}

/** The command that merges a task's branch by hand, or nothing when the host did not say where. */
internal fun mergeByHandCommand(branch:String,repository:String):String =
    if(branch.isBlank()||repository.isBlank()||branch.any {it.isWhitespace()||it=='"'}||repository.contains('"')) "" else " Command: git -C \"$repository\" merge $branch"

/** The first few conflicting file names, then how many more there are. */
internal fun conflictFiles(files:List<String>):String = files.take(5).joinToString(", ")+if(files.size>5)" and ${files.size-5} more" else ""

/** Task ids are UUIDs from the host; anything else is refused before a request is built. */
internal fun taskPath(id:String):String {require(Regex("[0-9a-fA-F-]{36}").matches(id)) {"Invalid task id"};return id}

/** What the pull request form sends. A fork owner makes the head `owner:branch`. */
data class PullRequestRequest(val path:String,val base:String,val branch:String,val forkOwner:String,val title:String,val body:String,val draft:Boolean) {
    fun head():String = forkOwner.trim().let {if(it.isEmpty()) branch.trim() else "$it:${branch.trim()}"}
}

/** `git rev-parse --show-toplevel HEAD --abbrev-ref HEAD` prints the branch on its third line; `HEAD` means detached. */
internal fun branchFromRevParse(out:String):String? = out.lines().getOrNull(2)?.trim()?.takeIf {it.isNotEmpty() && it!="HEAD"}
