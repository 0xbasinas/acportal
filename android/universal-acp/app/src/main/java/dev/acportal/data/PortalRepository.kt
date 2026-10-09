package dev.acportal.data

import dev.acportal.protocol.*
import dev.acportal.security.CredentialVault
import dev.acportal.storage.*
import dev.acportal.transport.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.*
import java.util.UUID

data class HostDetails(val host:HostProfile,val agents:List<AgentInfo>,val workspaces:List<Workspace>,val sessions:List<SessionInfo>,val recent:List<RecentWorkspace>)
class LiveSession(val host:HostProfile,info:SessionInfo,val transport:AgentTransport,initial:SessionState) {
    val metadata = MutableStateFlow(info)
    val info:SessionInfo get() = metadata.value
    val authenticating = MutableStateFlow(false)
    var authenticationRequestId:String? = null
    val state = MutableStateFlow(initial)
    val connection = MutableStateFlow<ConnectionState>(ConnectionState.Disconnected)
    val diagnostics = MutableStateFlow<List<ConnectionDiagnostic>>(emptyList())
    val localCopyGeneration = MutableStateFlow(0L)
    val attachments = MutableStateFlow<List<PromptAttachment>>(emptyList())
    val attachmentLock = Any()
    val promptSubmission = Mutex()
    @Volatile var submittedAttachments:Pair<String,List<PromptAttachment>>? = null
    val transportActions = Mutex()
    var manuallyDetached = false
    val jobs = mutableListOf<Job>()
    suspend fun close() { transport.disconnect();jobs.forEach { it.cancelAndJoin() };jobs.clear();attachments.value=emptyList();submittedAttachments=null }
}
class PortalRepository(
    private val dao:PortalDao, val api:HostApi,private val vault:CredentialVault,
    val settings:SettingsStore,private val scope:CoroutineScope,
) {
    val hosts = dao.hosts()
    val sessions = dao.sessions()
    val agentCatalogs=dao.agentCatalogs()
    val uiState=dao.uiState().map {it ?: StoredUiState()}
    suspend fun saveMainPage(page:String)=dao.saveMainPage(page)
    suspend fun saveSessionFilters(archived:Boolean,active:Boolean,query:String)=dao.saveSessionFilters(archived,active,query)
    private val active = mutableMapOf<String,LiveSession>()
    private val mutableActivities = MutableStateFlow<Map<String,LiveSessionActivity>>(emptyMap())
    val liveActivities:StateFlow<Map<String,LiveSessionActivity>> = mutableActivities.asStateFlow()
    private val lifecycle = Mutex()
    private val cacheStorage = Mutex()
    private val mcpStorage = Mutex()
    suspend fun mcpServers(hostId:String):List<McpDefinition> = withContext(Dispatchers.IO) {
        val alias=settings.mcpAliases.first()[hostId] ?: return@withContext emptyList()
        try {WireJson.decodeFromString<List<McpDefinition>>(vault.read(alias))}
        catch(cancelled:CancellationException) {throw cancelled}
        catch(failure:Exception) {throw IllegalStateException("Saved MCP servers could not be read. Retry after restoring this device's protected storage.",failure)}
    }
    suspend fun saveMcpServers(hostId:String,servers:List<McpDefinition>) = mcpStorage.withLock {withContext(Dispatchers.IO) {
        check(dao.host(hostId)!=null) {"Connection is no longer saved"}
        mcpWire(servers)
        val previous=settings.mcpAliases.first()[hostId]
        try {
            val alias=vault.save(WireJson.encodeToString(servers))
            try {settings.mcpAlias(hostId,alias)} catch(failure:Exception) {vault.remove(alias);throw failure}
        } catch(cancelled:CancellationException) {throw cancelled}
        catch(failure:Exception) {throw IllegalStateException("MCP servers could not be saved. Your changes are still in the editor. Retry when device storage is available.",failure)}
        previous?.let(vault::remove)
    }}
    suspend fun mcpHost(hostId:String):HostProfile = dao.host(hostId) ?: error("Connection is no longer saved")
    suspend fun workspaceAccess(hostId:String,path:String):WorkspaceAccess = settings.workspacePolicies.first()[workspacePolicyKey(hostId,path)] ?: WorkspaceAccess()
    suspend fun sessionAccess(hostId:String,id:String):WorkspaceAccess? = dao.session(hostId,id)?.let {storedSessionInfo(it).takeIf {info->info.status!=STORED_DETAILS_UNAVAILABLE}?.workspaceAccess}
    suspend fun saveWorkspaceAccess(hostId:String,path:String,value:WorkspaceAccess) {
        check(dao.host(hostId)!=null) {"Connection is no longer saved"}
        require(path.isNotBlank() && path.length<=4096) {"Choose a workspace"}
        settings.workspaceAccess(hostId,path,value)
    }
    suspend fun pair(address:String,code:String,label:String): HostProfile = withContext(Dispatchers.IO) {
        val base=validateAddress(address).toString().trimEnd('/')
        val result=api.pair(base,code,label.ifBlank { android.os.Build.MODEL })
        val old = dao.host(result.hostId)
        val alias=vault.save(result.token)
        val host=HostProfile(result.hostId,label.ifBlank { "Development host" },base,alias,result.deviceId,result.expiresAt)
        try { dao.saveHost(host) } catch (failure:Exception) { vault.remove(alias);throw failure }
        old?.let {
            lifecycle.withLock { active.filterKeys { key->key.startsWith("${it.id}/") }.keys.toList().forEach { key->closeActive(key) } }
            vault.remove(it.credentialAlias)
        }
        host
    }
    suspend fun details(hostId:String): HostDetails = withContext(Dispatchers.IO) {
        val host=dao.host(hostId) ?: error("Host is no longer saved")
        try {
            val status=api.status(host)
            check(status.hostId==host.id) { "Host identity changed. Pair the host again." }
            val agents=api.agents(host)
            val workspaces=api.workspaces(host)
            val sessions=api.sessions(host)
            val updated=host.copy(online=true,lastSeen=System.currentTimeMillis(),agentCount=agents.count { it.canStartSession() })
            dao.saveHost(updated)
            val catalog=WireJson.encodeToString(agents)
            require(utf8Bytes(catalog,STORED_CATALOG_MAX_BYTES)<=STORED_CATALOG_MAX_BYTES) {"Agent discovery is too large to save"}
            dao.saveAgentCatalog(StoredAgentCatalog(host.id,catalog,updated.lastSeen))
            for (session in sessions) saveMetadata(host.id,session)
            HostDetails(updated,agents,workspaces,sessions,dao.recent(hostId))
        } catch (cancelled:CancellationException) { throw cancelled }
        catch (failure:Exception) { dao.saveHost(host.copy(online=false));throw failure }
    }
    suspend fun refreshHosts() { hosts.first().forEach { host -> try { details(host.id) } catch (cancelled:CancellationException) { throw cancelled } catch (_:Exception) {} } }
    suspend fun browse(host:HostProfile,path:String): List<Workspace> = api.browse(host,path)
    suspend fun create(hostId:String,agentId:String,workspace:String,loadId:String?=null): StoredSession = withContext(Dispatchers.IO) {
        val host=dao.host(hostId) ?: error("Host is no longer saved")
        // Opt into form elicitation only on hosts that list the feature; older hosts reject unknown fields.
        val elicitation=try {HOST_FEATURE_FORM_ELICITATION in api.status(host).features} catch(cancelled:CancellationException) {throw cancelled} catch(_:Exception) {false}
        val created=api.create(host,agentId,workspace,loadId,mcpWire(mcpServers(hostId)),workspaceAccess(hostId,workspace),formElicitation=elicitation)
        dao.saveRecent(RecentWorkspace(hostId,workspace,agentId))
        saveMetadata(hostId,created)
    }
    private suspend fun saveMetadata(hostId:String,info:SessionInfo): StoredSession {
        val previous=dao.session(hostId,info.id)
        val metadata=storedMetadata(info)
        val stored=previous?.copy(metadata=metadata) ?: StoredSession(hostId,info.id,metadata)
        dao.saveSession(stored)
        return stored
    }
    suspend fun open(hostId:String,id:String): LiveSession = lifecycle.withLock {
        val key="$hostId/$id"
        active[key]?.let { return@withLock it }
        val stored=dao.session(hostId,id) ?: error("Session is no longer saved")
        val host=dao.host(hostId) ?: error("Host is no longer saved")
        val info=api.session(host,id)
        saveMetadata(hostId,info)
        check(info.status!="interrupted") { "The host restarted. Resume this session to continue." }
        val cached=storedSessionState(stored)
        val state=SessionReducer.limitMetadata(cached,cached.copy(replaying=true,modes=if(cached.modes.isEmpty())info.setup["modes"].objectValue() else cached.modes,
            models=if(cached.models.isEmpty())info.setup["models"].objectValue() else cached.models,
            configOptions=if(cached.configOptions.isEmpty())info.setup["configOptions"].arrayValue() else cached.configOptions))
        lateinit var live:LiveSession
        val transport=WebSocketTransport(api.client,host.address,api.token(host),id,{ live.state.value.sequence },scope)
        live=LiveSession(host,info,transport,state)
        active[key]=live
        mutableActivities.update {it+(key to liveSessionActivity(live.state.value,live.connection.value))}
        live.jobs += scope.launch {
            combine(live.state,live.connection,::liveSessionActivity).distinctUntilChanged().collect {activity->
                mutableActivities.update {it+(key to activity)}
            }
        }
        live.jobs += scope.launch {
            transport.messages().collect { bytes ->
                if (live.manuallyDetached) return@collect
                try {
                    val envelope=WireJson.parseToJsonElement(bytes.toString(Charsets.UTF_8)).objectValue()
                    live.state.update { SessionReducer.reduce(it,envelope) }
                    val message=envelope["message"].objectValue()
                    if(envelope["type"].text()=="session_closed")live.transportActions.withLock {live.manuallyDetached=true;transport.disconnect()}
                    if(message["method"]==null && message["id"].text()==live.authenticationRequestId) {
                        live.authenticationRequestId=null
                        live.authenticating.value=false
                        if(message["error"]==null)refreshMetadata(live)
                        else live.state.update {it.copy(errorOrigin=SessionErrorOrigin.AUTHENTICATION)}
                    } else if(live.info.acpSessionId.isBlank() && (envelope["type"].text()=="replay_complete" || message["result"]!=null))refreshMetadata(live)
                    if(envelope["direction"].text()=="client" && message["method"].text()=="session/prompt") {
                        val sent=live.submittedAttachments
                        if(sent?.first==message["id"].text()) {live.attachments.compareAndSet(sent.second,emptyList());live.submittedAttachments=null}
                        dao.saveDraft(hostId,id,"")
                    }
                    if (envelope["type"].text()=="reconnect_required") live.transportActions.withLock {
                        if(!live.manuallyDetached) {transport.disconnect();transport.connect()}
                    }
                } catch (cancelled:CancellationException) { throw cancelled }
                catch (_:PendingPermissionLimitException) {
                    live.transportActions.withLock {
                        live.manuallyDetached=true
                        transport.disconnect()
                        live.state.update { it.copy(replaying=true,error="Pending approvals exceeded the client limit. Connection detached; no decision was sent. Review the host session before reconnecting.",errorOrigin=SessionErrorOrigin.PROTOCOL) }
                    }
                }
                catch (_:Exception) { live.state.update { it.copy(error="An invalid host event was received.",errorOrigin=SessionErrorOrigin.PROTOCOL) } }
            }
        }
        live.jobs += scope.launch { transport.connectionState().collect {
            if(it!=ConnectionState.Connected)live.state.update {state->state.copy(replaying=true)}
            live.connection.value=it
            live.diagnostics.update { events->appendDiagnostic(events,connectionDiagnostic(it)) }
            if(it is ConnectionState.Failed) {
                try {refreshMetadata(live)} catch(cancelled:CancellationException) {throw cancelled} catch(_:Exception) {}
            }
        } }
        live.jobs += scope.launch {
            var lastWritten=-1L
            while (isActive) {
                delay(500)
                cacheStorage.withLock {
                    val current=live.state.value
                    if(current.sequence!=lastWritten) {
                        val cache=settings.settings.first().cacheMessages
                        dao.saveState(hostId,id,sessionCache(current,cache),System.currentTimeMillis())
                        lastWritten=current.sequence
                    }
                }
            }
        }
        transport.connect()
        live
    }
    private suspend fun refreshMetadata(live:LiveSession) {
        val info=api.session(live.host,live.info.id)
        val becameReady=live.info.acpSessionId.isBlank() && info.acpSessionId.isNotBlank()
        live.metadata.value=info
        saveMetadata(live.host.id,info)
        live.state.update { SessionReducer.limitMetadata(it,it.copy(modes=info.setup["modes"].objectValue(),models=info.setup["models"].objectValue(),configOptions=info.setup["configOptions"].arrayValue(),error=if(becameReady)null else it.error)) }
    }
    suspend fun reconnect(live:LiveSession) = live.transportActions.withLock { check(!live.state.value.sessionClosed) {"This agent session stopped. Open Sessions to continue."};live.manuallyDetached=false;live.transport.disconnect();live.transport.connect() }
    suspend fun disconnect(live:LiveSession) = live.transportActions.withLock { live.manuallyDetached=true;live.transport.disconnect() }
    fun addAttachment(live:LiveSession,attachment:PromptAttachment,version:Long=live.localCopyGeneration.value) = synchronized(live.attachmentLock) {
        require(version==live.localCopyGeneration.value) {"File selection expired. Choose the file again."}
        live.attachments.update {checkedAttachments(live.info,it+attachment)}
    }
    fun removeAttachment(live:LiveSession,index:Int) {live.attachments.update {it.filterIndexed {position,_->position!=index}}}
    suspend fun prompt(live:LiveSession,text:String,resourceUri:String?=null,attachments:List<PromptAttachment> = live.attachments.value) = live.promptSubmission.withLock {
        check(!live.state.value.sessionClosed && live.info.status!="exited") {"This agent session stopped. Open Sessions to continue."}
        check(!live.state.value.processing && !live.state.value.replaying) { "Wait for the current turn to finish" }
        val info=live.info
        val generation=live.localCopyGeneration.value
        val id=UUID.randomUUID().toString()
        val bytes=withContext(Dispatchers.Default) {
            // Reject obviously oversized text before creating a JSON string and UTF-8 copy.
            require(text.length<=MAX_PROMPT_WIRE_BYTES) {"This prompt is too large. Shorten the message or remove an attachment."}
            val attached=checkedAttachments(info,attachments+(resourceUri?.let {listOf(contextReference(it))} ?: emptyList()))
            require(text.isNotBlank() || attached.isNotEmpty()) {"Enter a message or attach a file."}
            boundedJsonBytes(request(id,"session/prompt",buildJsonObject {
                put("sessionId",info.acpSessionId)
                put("prompt",promptContent(text,attached))
            }),MAX_PROMPT_WIRE_BYTES,"This prompt is too large. Shorten the message or remove an attachment.")
        }
        // Preparation suspends the caller. Recheck execution and local-copy ownership
        // before sending; reconnect/clear/stop must never revive a prepared submission.
        check(!live.state.value.sessionClosed && live.info.status!="exited" && !live.manuallyDetached && !live.state.value.processing && !live.state.value.replaying) {"Wait for the session to reconnect before sending."}
        check(live.info.acpSessionId==info.acpSessionId && live.localCopyGeneration.value==generation) {"File selection expired. Choose the file again."}
        live.submittedAttachments=id to attachments
        live.state.update {it.copy(processing=true,promptRequestId=JsonPrimitive(id).toString())}
        try {live.transport.send(bytes)}
        catch(failure:Exception) {live.state.update {it.copy(processing=false,promptRequestId=null)};live.submittedAttachments=null;throw failure}
    }
    suspend fun cancel(live:LiveSession) {
        val state=live.state.value
        check(!state.sessionClosed && state.processing && !state.replaying && live.connection.value==ConnectionState.Connected && !live.manuallyDetached) {"Wait for the active turn to reconnect before stopping it."}
        live.transport.send(cancel(live.info.acpSessionId).toString().toByteArray())
    }
    fun dismissSessionError(live:LiveSession) {live.state.update {it.copy(error=null)}}
    suspend fun permission(live:LiveSession,permission:Permission,optionId:String) {
        val state=live.state.value
        check(!state.sessionClosed && !state.replaying && live.connection.value==ConnectionState.Connected && !live.manuallyDetached) {"Wait for the session to reconnect before choosing a permission."}
        val current=state.permissions[permission.id.toString()]
        check(current==permission) {"This permission request is no longer pending. Review the current request."}
        live.transport.send(permissionResponse(permission,optionId).toString().toByteArray())
    }
    /** Answers a pending form elicitation: accept (with content), decline or cancel. */
    suspend fun elicitation(live:LiveSession,permission:Permission,action:String,content:JsonObject?) {
        val state=live.state.value
        check(!state.sessionClosed && !state.replaying && live.connection.value==ConnectionState.Connected && !live.manuallyDetached) {"Wait for the session to reconnect before answering."}
        val current=state.permissions[permission.id.toString()]
        check(current==permission && permission.isElicitation) {"This question is no longer pending. Review the current request."}
        live.transport.send(elicitationResponse(permission,action,content).toString().toByteArray())
    }
    suspend fun configure(live:LiveSession,method:String,params:JsonObject) {
        val id=UUID.randomUUID().toString()
        if(method=="authenticate") {
            check(!live.authenticating.value) { "Sign-in is already in progress" }
            check(live.info.authenticationMethods().any { it["id"]==params["methodId"] }) { "Sign-in method is unavailable" }
            live.authenticationRequestId=id
            live.authenticating.value=true
        }
        try {
            val scoped=if(method.startsWith("session/"))JsonObject(params+("sessionId" to JsonPrimitive(live.info.acpSessionId)))else params
            live.transport.send(request(id,method,scoped).toString().toByteArray())
        } catch(failure:Exception) {
            if(method=="authenticate") { live.authenticationRequestId=null;live.authenticating.value=false }
            throw failure
        }
    }
    suspend fun draft(hostId:String,id:String,text:String) = cacheStorage.withLock {dao.saveDraft(hostId,id,storedDraft(text))}
    suspend fun removeLocalCopy(live:LiveSession) = cacheStorage.withLock {
        var previous:SessionState
        var cleared:SessionState
        do {previous=live.state.value;cleared=previous.copy(items=emptyList(),terminals=emptyMap(),localHistoryCleared=true)} while(!live.state.compareAndSet(previous,cleared))
        try {dao.clearLocalCopy(live.host.id,live.info.id,WireJson.encodeToString(SessionState(sequence=cleared.sequence,localHistoryCleared=true)),System.currentTimeMillis())}
        catch(cancelled:CancellationException) {live.state.compareAndSet(cleared,previous);throw cancelled}
        catch(failure:Exception) {live.state.compareAndSet(cleared,previous);throw IllegalStateException("Local messages could not be removed. Try again.",failure)}
        synchronized(live.attachmentLock) {live.attachments.value=emptyList();live.localCopyGeneration.update {it+1}}
    }
    suspend fun archive(stored:StoredSession) { dao.archive(stored.hostId,stored.id,!stored.archived) }
    suspend fun delete(stored:StoredSession) {
        val host=dao.host(stored.hostId) ?: error("Host is no longer saved")
        api.delete(host,stored.id)
        lifecycle.withLock { closeActive("${stored.hostId}/${stored.id}") }
        dao.deleteSession(stored.hostId,stored.id)
    }
    suspend fun forget(hostId:String) {
        val host=dao.host(hostId) ?: return
        api.revoke(host)
        lifecycle.withLock { active.filterKeys { it.startsWith("$hostId/") }.keys.toList().forEach { key -> closeActive(key) } }
        dao.deleteRecent(hostId);dao.deleteHost(hostId);vault.remove(host.credentialAlias)
        mcpStorage.withLock {val alias=settings.mcpAliases.first()[hostId];settings.mcpAlias(hostId,null);alias?.let(vault::remove)}
        settings.removeWorkspaceAccess(hostId)
    }
    suspend fun clearCache() = lifecycle.withLock {cacheStorage.withLock {
        val records=dao.sessions().first()
        for(record in records) {
            val current=active["${record.hostId}/${record.id}"]?.state?.value ?: storedSessionState(record)
            dao.saveState(record.hostId,record.id,WireJson.encodeToString(SessionState(sequence=current.sequence,historyGap=current.historyGap,localHistoryCleared=true)),record.updatedAt)
        }
    }}
    private suspend fun closeActive(key:String) {
        active.remove(key)?.close()
        mutableActivities.update {it-key}
    }
}

