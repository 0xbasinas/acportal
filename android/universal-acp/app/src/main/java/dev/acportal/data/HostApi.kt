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
    suspend fun sessions(host:HostProfile): List<SessionInfo> = WireJson.decodeFromJsonElement(execute(host.address,"/v1/sessions",vault.read(host.credentialAlias)))
    suspend fun session(host:HostProfile,id:String): SessionInfo = WireJson.decodeFromJsonElement(execute(host.address,"/v1/sessions/$id",vault.read(host.credentialAlias)))
    suspend fun create(host:HostProfile,agentId:String,workspace:String,loadId:String?=null,mcpServers:JsonArray=JsonArray(emptyList()),access:WorkspaceAccess=WorkspaceAccess(),formElicitation:Boolean=false): SessionInfo = WireJson.decodeFromJsonElement(execute(host.address,"/v1/sessions",vault.read(host.credentialAlias),buildJsonObject {
        put("agentId",agentId);put("workspace",workspace);loadId?.let { put("loadSessionId",it) }
        put("mcpServers",mcpServers)
        put("workspaceAccess",buildJsonObject {put("readFiles",access.readFiles);put("writeFiles",access.writeFiles);put("terminal",access.terminal);if(formElicitation)put("formElicitation",true)})
    },"POST"))
    suspend fun delete(host:HostProfile,id:String) { execute(host.address,"/v1/sessions/$id",vault.read(host.credentialAlias),method="DELETE") }
    suspend fun revoke(host:HostProfile) { execute(host.address,"/v1/device",vault.read(host.credentialAlias),method="DELETE") }
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
                        val code=runCatching {WireJson.parseToJsonElement(response.peekBody(1024).string()).objectValue()["error"].objectValue()["code"].text()}.getOrDefault("")
                        error(hostRequestError(response.code,code,response.request.method,response.request.url.encodedPath))
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

internal fun hostRequestError(status:Int,code:String,method:String,path:String):String = when {
    code=="unsupported_mcp_transport" -> "This agent does not support an enabled MCP server's transport. Disable that server or choose another transport."
    status==401 && method=="POST" && path=="/v1/pair" -> "Pairing code was rejected. It may have expired or already been used. Run acpd pair on the host for a new code."
    status==401 -> "Credentials expired or pairing failed. Pair this host again."
    status==409 -> "This session already has a controller."
    status==400 && method=="POST" && path=="/v1/sessions" -> "The host could not start this session. Choose an existing allowed folder and check the selected agent and MCP settings, then press Start session again."
    else -> "Host request failed ($status)."
}
