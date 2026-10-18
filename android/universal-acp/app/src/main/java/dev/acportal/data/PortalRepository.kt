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

data class HostDetails(val host:HostProfile,val agents:List<AgentInfo>,val workspaces:List<Workspace>,val sessions:List<SessionInfo>,val recent:List<RecentWorkspace>,val features:List<String> = emptyList())
/** A draft from another device: apply it only while the composer still shows [replaces]. */
data class RemoteDraft(val text:String,val replaces:String,val at:Long)
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
    /** The host keeps drafts and prompt history for the desktop too (feature `sharedDrafts`). */
    @Volatile var sharedDrafts = false
    val sharedHistory = MutableStateFlow<List<String>>(emptyList())
    var draftUpload:Job? = null
    /** The draft text known to match the host copy, and a newer host draft for an unedited composer. */
    @Volatile var syncedDraft = ""
    @Volatile var remoteDraftAt = 0L
    val remoteDraft = MutableStateFlow<RemoteDraft?>(null)
    suspend fun close() { transport.disconnect();jobs.forEach { it.cancelAndJoin() };jobs.clear();attachments.value=emptyList();submittedAttachments=null }
}
class PortalRepository(
    private val dao:PortalDao, val api:HostApi,private val vault:CredentialVault,
    val settings:SettingsStore,private val scope:CoroutineScope,
) {
    /** Session titles kept on the host (`hostId/sessionId` to title), for sessions this phone has no copy of yet. */
    val sharedTitles = MutableStateFlow<Map<String,String>>(emptyMap())
    private fun applyShared(hostId:String,shared:JsonObject) {
        val titles=shared["titles"].objectValue().mapNotNull {(id,title)->(title as? JsonPrimitive)?.contentOrNull?.let {"$hostId/$id" to it}}.toMap()
        sharedTitles.update {current->current.filterKeys {!it.startsWith("$hostId/")}+titles}
    }
    val hosts = dao.hosts()
    val sessions = dao.sessions()
    val agentCatalogs=dao.agentCatalogs()
    val uiState=dao.uiState().map {it ?: StoredUiState()}
    suspend fun saveMainPage(page:String)=dao.saveMainPage(page)
    suspend fun saveSessionFilters(archived:Boolean,active:Boolean,query:String)=dao.saveSessionFilters(archived,active,query)
    private val active = mutableMapOf<String,LiveSession>()
    /** How long each turn took, measured here while a turn ran; persisted so rows can show time worked. */
    val turnTimes = MutableStateFlow<TurnTimes>(emptyMap())
    init {scope.launch {try {turnTimes.update {current->settings.turnTimes.first().let {saved->saved+current}}} catch(cancelled:CancellationException) {throw cancelled} catch(_:Exception) {}}}
    private fun recordTurn(change:(TurnTimes)->TurnTimes) {
        val previous=turnTimes.value
        val next=turnTimes.updateAndGet(change)
        if(next!==previous)scope.launch {try {settings.saveTurnTimes(next)} catch(cancelled:CancellationException) {throw cancelled} catch(_:Exception) {}}
    }
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
            // Update status columns only. Saving the whole snapshot read before these slow
            // requests would restore a credential alias that a concurrent re-pair replaced.
            val seen=System.currentTimeMillis();val count=agents.count { it.canStartSession() }
            dao.markHostOnline(host.id,seen,count)
            val updated=dao.host(host.id) ?: host.copy(online=true,lastSeen=seen,agentCount=count)
            val catalog=WireJson.encodeToString(agents)
            require(utf8Bytes(catalog,STORED_CATALOG_MAX_BYTES)<=STORED_CATALOG_MAX_BYTES) {"Agent discovery is too large to save"}
            dao.saveAgentCatalog(StoredAgentCatalog(host.id,catalog,updated.lastSeen))
            for (session in sessions) saveMetadata(host.id,session)
            try {settings.saveHostProjects(host.id,HostProjects(workspaces.map {it.path},status.scratchRoot))} catch(cancelled:CancellationException) {throw cancelled} catch(_:Exception) {}
            if(HOST_FEATURE_SHARED_DRAFTS in status.features)try {applyShared(host.id,api.shared(host))} catch(cancelled:CancellationException) {throw cancelled} catch(_:Exception) {}
            HostDetails(updated,agents,workspaces,sessions,dao.recent(hostId),status.features)
        } catch (cancelled:CancellationException) { throw cancelled }
        catch (failure:Exception) { dao.markHostOffline(host.id);throw failure }
    }
    private suspend fun savedHost(hostId:String)=dao.host(hostId) ?: error("Host is no longer saved")
    suspend fun tasks(hostId:String):List<TaskSummary> = withContext(Dispatchers.IO) {api.tasks(savedHost(hostId))}
    suspend fun task(hostId:String,id:String):TaskDetail = withContext(Dispatchers.IO) {api.task(savedHost(hostId),id)}
    suspend fun discardTask(hostId:String,id:String) = withContext(Dispatchers.IO) {api.discardTask(savedHost(hostId),id)}
    suspend fun mergeTask(hostId:String,id:String) = withContext(Dispatchers.IO) {api.mergeTask(savedHost(hostId),id)}
    suspend fun cancelTask(hostId:String,id:String) = withContext(Dispatchers.IO) {api.cancelTask(savedHost(hostId),id)}
    suspend fun setTaskSupervision(hostId:String,id:String,paused:Boolean) = withContext(Dispatchers.IO) {api.setTaskSupervision(savedHost(hostId),id,paused)}
    suspend fun refreshHosts() { hosts.first().forEach { host -> try { details(host.id) } catch (cancelled:CancellationException) { throw cancelled } catch (_:Exception) {} } }
    suspend fun browse(host:HostProfile,path:String): List<Workspace> = api.browse(host,path)
    fun agentClient(host:HostProfile)=object:AgentClient {
        override suspend fun discover()=api.discoverAgents(host)
        override suspend fun check(command:String,env:Map<String,String>)=api.checkAgent(host,command,env)
        override suspend fun add(registration:AgentRegistration) {api.addAgent(host,registration)}
        override suspend fun remove(id:String) {api.removeAgent(host,id)}
    }
    /** Folder choosing and adding for one host; the phone is paired, so the host accepts these. */
    fun folderClient(host:HostProfile)=object:FolderClient {
        override suspend fun directories(path:String?)=api.directories(host,path)
        override suspend fun add(path:String)=api.addWorkspace(host,path)
        override suspend fun remove(path:String)=api.removeWorkspace(host,path)
    }
    suspend fun earlierSessions(hostId:String,agentId:String,workspace:String): EarlierSessions = withContext(Dispatchers.IO) {api.earlierSessions(dao.host(hostId) ?: error("Host is no longer saved"),agentId,workspace)}
    suspend fun git(hostId:String,path:String,op:String,params:Map<String,String>,input:String?=null):Result<String> = withContext(Dispatchers.IO) {api.git(dao.host(hostId) ?: error("Host is no longer saved"),path,op,params,input)}
    suspend fun currentBranch(hostId:String,path:String): String? = withContext(Dispatchers.IO) {api.currentBranch(dao.host(hostId) ?: error("Host is no longer saved"),path)}
    suspend fun openPullRequest(hostId:String,request:PullRequestRequest): String = withContext(Dispatchers.IO) {api.openPullRequest(dao.host(hostId) ?: error("Host is no longer saved"),request)}
    suspend fun presets(hostId:String,agentId:String): List<PermissionPreset> = withContext(Dispatchers.IO) {api.presets(dao.host(hostId) ?: error("Host is no longer saved"),agentId)}
    suspend fun create(hostId:String,agentId:String,workspace:String,loadId:String?=null,askEverything:Boolean=false,yolo:Boolean=false): StoredSession = withContext(Dispatchers.IO) {
        val host=dao.host(hostId) ?: error("Host is no longer saved")
        // Opt into form elicitation and manual approval only on hosts that list the feature; older hosts reject unknown fields.
        val features=try {api.status(host).features} catch(cancelled:CancellationException) {throw cancelled} catch(_:Exception) {emptyList()}
        val elicitation=HOST_FEATURE_FORM_ELICITATION in features
        val manual=askEverything && HOST_FEATURE_MANUAL_APPROVAL in features
        val booleans=HOST_FEATURE_BOOLEAN_OPTIONS in features
        val created=api.create(host,agentId,workspace,loadId,mcpWire(mcpServers(hostId)),workspaceAccess(hostId,workspace),formElicitation=elicitation,manualApproval=manual,booleanOptions=booleans,yolo=yolo && HOST_FEATURE_PRESETS in features)
        dao.saveRecent(RecentWorkspace(hostId,workspace,agentId))
        saveMetadata(hostId,created)
    }
    private suspend fun saveMetadata(hostId:String,info:SessionInfo): StoredSession {
        val previous=dao.session(hostId,info.id)
        val metadata=storedMetadata(info)
        val stored=previous?.copy(metadata=metadata) ?: StoredSession(hostId,info.id,metadata)
        dao.saveSessionMetadata(stored)
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
        // Continue a draft typed on the desktop when this phone has none; a local draft wins and is uploaded.
        val shared=try {api.shared(host)} catch(cancelled:CancellationException) {throw cancelled} catch(_:Exception) {null}
        val remoteDraft=shared?.get("drafts").objectValue()[id].objectValue()["text"].text()
        if(remoteDraft.isNotBlank() && stored.draft.isBlank())dao.saveDraft(hostId,id,storedDraft(remoteDraft))
        shared?.let {applyShared(hostId,it)}
        lateinit var live:LiveSession
        val transport=WebSocketTransport(api.client,host.address,api.token(host),id,{ live.state.value.sequence },scope)
        live=LiveSession(host,info,transport,state)
        if(shared!=null) {
            live.sharedDrafts=true
            live.syncedDraft=if(stored.draft.isBlank())remoteDraft else stored.draft
            live.remoteDraftAt=shared["drafts"].objectValue()[id].objectValue()["updatedAt"].text().toLongOrNull() ?: 0
            applySharedToLive(live,shared)
            // Follow drafts typed and prompts sent on the desktop while this session is open here.
            live.jobs += scope.launch {
                while(isActive) {
                    delay(SHARED_POLL_MS)
                    try {applySharedToLive(live,api.shared(host).also {applyShared(hostId,it)})} catch(cancelled:CancellationException) {throw cancelled} catch(_:Exception) {}
                }
            }
        }
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
                    var before=live.state.value
                    live.state.update { before=it;SessionReducer.reduce(it,envelope) }
                    // Time the turn this event starts or ends: by the host's timestamp when it sends one (so a
                    // replayed turn counts), by this phone's clock for a live event from an older host.
                    val hostTime=(envelope["at"] as? JsonPrimitive)?.longOrNull?.takeIf {it>0}
                    val at=hostTime ?: if(before.replaying)null else System.currentTimeMillis()
                    val after=live.state.value
                    if(at!=null)recordTurn {TurnClock.observe(it,key,before,after,at)}
                    val message=envelope["message"].objectValue()
                    if(envelope["type"].text()=="control_taken")transport.controlTaken()
                    if(envelope["type"].text()=="session_closed")live.transportActions.withLock {live.manuallyDetached=true;transport.disconnect()}
                    if(message["method"]==null && message["id"].text()==live.authenticationRequestId) {
                        live.authenticationRequestId=null
                        live.authenticating.value=false
                        if(message["error"]==null)refreshMetadata(live)
                        else live.state.update {it.copy(errorOrigin=SessionErrorOrigin.AUTHENTICATION)}
                    } else if(live.info.acpSessionId.isBlank() && (envelope["type"].text()=="replay_complete" || message["result"]!=null))refreshMetadata(live)
                    if(envelope["direction"].text()=="client" && message["method"].text()=="session/prompt") {
                        val sent=live.submittedAttachments
                        // A replayed prompt from an earlier controller/process is not
                        // an acknowledgement of this device's current unsent draft.
                        if(sent?.first==message["id"].text()) {
                            live.attachments.compareAndSet(sent.second,emptyList())
                            live.submittedAttachments=null
                            dao.saveDraft(hostId,id,"")
                            uploadDraft(live,"")
                        }
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
    /** Continue here a session another device (desktop or phone) controls; history replays from the cursor. */
    suspend fun takeControl(live:LiveSession) = live.transportActions.withLock { check(!live.state.value.sessionClosed) {"This agent session stopped. Open Sessions to continue."};live.manuallyDetached=false;live.transport.disconnect();live.transport.requestTakeover();live.transport.connect() }
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
        shareHistory(live,text)
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
    suspend fun draft(hostId:String,id:String,text:String) {
        cacheStorage.withLock {dao.saveDraft(hostId,id,storedDraft(text))}
        active["$hostId/$id"]?.let {uploadDraft(it,text)}
    }
    private suspend fun applySharedToLive(live:LiveSession,shared:JsonObject) {
        live.sharedHistory.value=(shared["history"].objectValue()[live.info.workspace] as? JsonArray)?.mapNotNull {(it as? JsonPrimitive)?.contentOrNull}.orEmpty()
        val entry=shared["drafts"].objectValue()[live.info.id] as? JsonObject
        val at=entry?.get("updatedAt").text().toLongOrNull() ?: 0
        val text=entry?.get("text").text()
        if(at<=live.remoteDraftAt && entry!=null)return
        if(entry==null && live.syncedDraft.isEmpty())return
        // Only an unedited composer follows the other device; local typing wins and is uploaded.
        val local=dao.session(live.host.id,live.info.id)?.draft.orEmpty()
        if(local!=live.syncedDraft || text==local) {live.remoteDraftAt=at;if(text==local)live.syncedDraft=text;return}
        cacheStorage.withLock {dao.saveDraft(live.host.id,live.info.id,storedDraft(text))}
        live.remoteDraft.value=RemoteDraft(text,live.syncedDraft,at)
        live.syncedDraft=text
        live.remoteDraftAt=at
    }
    /** Debounced, best-effort upload; a failure leaves the local draft as it is. */
    private fun uploadDraft(live:LiveSession,text:String) {
        if(!live.sharedDrafts || utf8Bytes(text,SHARED_DRAFT_MAX_BYTES)>SHARED_DRAFT_MAX_BYTES)return
        synchronized(live) {
            live.draftUpload?.cancel()
            live.draftUpload=scope.launch {
                delay(if(text.isEmpty())0 else 800)
                try {api.putDraft(live.host,live.info.id,text);live.syncedDraft=text} catch(cancelled:CancellationException) {throw cancelled} catch(_:Exception) {}
            }
        }
    }
    private fun shareHistory(live:LiveSession,text:String) {
        if(!live.sharedDrafts || text.isBlank() || utf8Bytes(text,SHARED_PROMPT_MAX_BYTES)>SHARED_PROMPT_MAX_BYTES)return
        live.sharedHistory.update {(listOf(text)+it.filter {prompt->prompt!=text}).take(50)}
        val key="${live.host.id}/${live.info.id}"
        val title=text.lineSequence().first().trim().take(60)
        val needsTitle=title.isNotEmpty() && key !in sharedTitles.value
        if(needsTitle)sharedTitles.update {it+(key to title)}
        scope.launch {
            try {
                api.addHistory(live.host,live.info.workspace,text)
                // The desktop names sessions by their first prompt; do the same for one started here.
                if(needsTitle)api.putTitle(live.host,live.info.id,title)
            } catch(cancelled:CancellationException) {throw cancelled} catch(_:Exception) {}
        }
    }
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

