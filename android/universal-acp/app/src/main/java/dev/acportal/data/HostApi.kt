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
    suspend fun locateAgents(host:HostProfile,commands:List<String>): List<LocatedAgent> = WireJson.decodeFromJsonElement(execute(host.address,"/v1/agents/locate",vault.read(host.credentialAlias),buildJsonObject {put("commands",JsonArray(commands.map {JsonPrimitive(it)}))},"POST"))
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
    suspend fun create(host:HostProfile,agentId:String,workspace:String,loadId:String?=null,mcpServers:JsonArray=JsonArray(emptyList()),access:WorkspaceAccess=WorkspaceAccess(),formElicitation:Boolean=false): SessionInfo = WireJson.decodeFromJsonElement(execute(host.address,"/v1/sessions",vault.read(host.credentialAlias),buildJsonObject {
        put("agentId",agentId);put("workspace",workspace);loadId?.let { put("loadSessionId",it) }
        put("mcpServers",mcpServers)
        put("workspaceAccess",buildJsonObject {put("readFiles",access.readFiles);put("writeFiles",access.writeFiles);put("terminal",access.terminal);if(formElicitation)put("formElicitation",true)})
    },"POST"))
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
                        error(hostRequestError(response.code,code,response.request.method,response.request.url.encodedPath,files,branch,repository))
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

internal fun hostRequestError(status:Int,code:String,method:String,path:String,files:List<String> = emptyList(),branch:String="",repository:String=""):String = when {
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
