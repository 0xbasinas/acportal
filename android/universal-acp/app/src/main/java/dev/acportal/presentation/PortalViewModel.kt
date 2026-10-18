package dev.acportal.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.acportal.data.*
import dev.acportal.protocol.*
import dev.acportal.storage.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.serialization.json.*

data class WorkspaceBrowseState(val hostId:String?=null,val path:String?=null,val loading:Boolean=false,val folders:List<Workspace>?=null,val error:String?=null)
data class PortalUiState(val loading:Boolean=false,val error:String?=null,val errorRetry:(()->Unit)?=null,val details:HostDetails?=null,val live:LiveSession?=null,val workspaceBrowse:WorkspaceBrowseState=WorkspaceBrowseState(),val mcpHost:HostProfile?=null,val mcpServers:List<McpDefinition> = emptyList(),val accessHost:HostProfile?=null,val accessPath:String?=null,val savedAccess:WorkspaceAccess=WorkspaceAccess(),val currentAccess:WorkspaceAccess?=null)
/** The delegated task tree of one host (experimental; only shown when the host lists the `orchestrator` feature). */
data class TasksUiState(val hostId:String?=null,val tasks:List<TaskSummary>?=null,val detail:TaskDetail?=null,val loading:Boolean=false,val error:String?=null,val notice:String?=null)
class PortalViewModel(val repository:PortalRepository):ViewModel() {
    val hosts=repository.hosts.stateIn(viewModelScope,SharingStarted.WhileSubscribed(5000),emptyList())
    val sessions=repository.sessions.stateIn(viewModelScope,SharingStarted.WhileSubscribed(5000),emptyList())
    private val summaryMemo=SessionListSummaries()
    /** Row titles, times and search text, decoded off the main thread and only for changed rows. */
    val sharedTitles=repository.sharedTitles.asStateFlow()
    val sessionSummaries=sessions.map {summaryMemo.update(it)}.flowOn(Dispatchers.Default).stateIn(viewModelScope,SharingStarted.WhileSubscribed(5000),emptyMap())
    val liveActivities=repository.liveActivities
    val turnTimes=repository.turnTimes.asStateFlow()
    val hostProjects=repository.settings.hostProjects.stateIn(viewModelScope,SharingStarted.WhileSubscribed(5000),emptyMap())
    val agentCatalogs=repository.agentCatalogs.stateIn(viewModelScope,SharingStarted.WhileSubscribed(5000),emptyList())
    val savedUiState=repository.uiState.stateIn(viewModelScope,SharingStarted.Eagerly,null)
    fun mainPage(page:String) {viewModelScope.launch {repository.saveMainPage(page)}}
    fun sessionFilters(archived:Boolean,active:Boolean,query:String) {viewModelScope.launch {repository.saveSessionFilters(archived,active,query)}}
    val settings=repository.settings.settings.stateIn(viewModelScope,SharingStarted.WhileSubscribed(5000),PortalSettings())
    val ui=MutableStateFlow(PortalUiState())
    private val mcpEdits=mutableMapOf<String,McpEditState>()
    fun mcpEdit(hostId:String)=mcpEdits.getOrPut(hostId) {McpEditState()}
    private var browseJob:Job?=null
    private var browseVersion=0L
    private var detailsJob:Job?=null
    private var detailsVersion=0L
    init { refresh() }
    // Main-thread counters: one finished operation must not clear loading while another
    // (for example a slow session creation) is still running.
    private var activeWork=0
    private var detailsLoading=false
    private fun publishLoading() {ui.update {it.copy(loading=activeWork>0 || detailsLoading)}}
    private val creating=mutableSetOf<String>()
    private fun work(onRetry:(()->Unit)?=null,action:suspend ()->Unit) { viewModelScope.launch { activeWork++;ui.update { it.copy(loading=true,error=null,errorRetry=null) };try { action() } catch (cancelled:CancellationException) { throw cancelled } catch (failure:Exception) { ui.update { it.copy(error=failure.message ?: "Something went wrong. Try again.",errorRetry=onRetry) } } finally { activeWork--;publishLoading() } } }
    fun refresh() = work { repository.refreshHosts() }
    fun details(id:String) {
        cancelBrowse()
        val version=++detailsVersion
        detailsJob?.cancel()
        detailsLoading=true
        ui.update {it.copy(details=null,loading=true,error=null,errorRetry=null)}
        detailsJob=viewModelScope.launch {
            try {
                val details=repository.details(id)
                if(version==detailsVersion)ui.update {it.copy(details=details)}
            } catch(cancelled:CancellationException) {throw cancelled}
            catch(failure:Exception) {if(version==detailsVersion)ui.update {it.copy(error=failure.message ?: "Check the host connection and try again.",errorRetry={details(id)})}}
            finally {if(version==detailsVersion) {detailsLoading=false;publishLoading()}}
        }
    }
    fun pair(address:String,code:String,label:String,onDone:()->Unit) = work { repository.pair(address,code,label);repository.refreshHosts();onDone() }
    fun folderClient(host:HostProfile)=repository.folderClient(host)
    fun agentClient(host:HostProfile)=repository.agentClient(host)
    fun removeAgent(host:HostProfile,id:String,onDone:()->Unit) = work {repository.agentClient(host).remove(id);onDone()}
    fun cancelBrowse() {
        browseVersion++
        browseJob?.cancel();browseJob=null
        ui.update {it.copy(workspaceBrowse=WorkspaceBrowseState())}
    }
    fun browse(host:HostProfile,path:String) {
        cancelBrowse()
        val version=browseVersion
        ui.update {it.copy(workspaceBrowse=WorkspaceBrowseState(host.id,path,loading=true))}
        browseJob=viewModelScope.launch {
            try {
                val folders=repository.browse(host,path)
                if(version==browseVersion)ui.update {it.copy(workspaceBrowse=WorkspaceBrowseState(host.id,path,folders=folders))}
            } catch(cancelled:CancellationException) {throw cancelled}
            catch(failure:Exception) {
                if(version==browseVersion)ui.update {it.copy(workspaceBrowse=WorkspaceBrowseState(host.id,path,error=failure.message ?: "Check the host connection and try again."))}
            }
        }
    }
    /** A repeated request for the same host, agent and folder while one is in flight is ignored. */
    /** Reopens a session the agent remembers (`session/load`), keeping the Ask for everything choice. */
    suspend fun presets(hostId:String,agentId:String): List<PermissionPreset>? = try {repository.presets(hostId,agentId)} catch(cancelled:kotlinx.coroutines.CancellationException) {throw cancelled} catch(_:Exception) {null}
    fun createEarlier(hostId:String,agentId:String,workspace:String,sessionId:String,askEverything:Boolean,onDone:(StoredSession)->Unit,yolo:Boolean=false) = work {
        onDone(repository.create(hostId,agentId,workspace,sessionId,askEverything,yolo))
    }
    suspend fun earlierSessions(hostId:String,agentId:String,workspace:String): EarlierSessions? = try {repository.earlierSessions(hostId,agentId,workspace)} catch(cancelled:kotlinx.coroutines.CancellationException) {throw cancelled} catch(_:Exception) {null}
    suspend fun openTerminal(hostId:String,path:String,rows:Int,cols:Int):Result<String> = try {Result.success(repository.openTerminal(hostId,path,rows,cols))} catch(cancelled:kotlinx.coroutines.CancellationException) {throw cancelled} catch(failure:Exception) {Result.failure(failure)}
    fun endTerminal(hostId:String,id:String) { viewModelScope.launch {try {repository.endTerminal(hostId,id)} catch(cancelled:kotlinx.coroutines.CancellationException) {throw cancelled} catch(_:Exception) {}} }
    suspend fun terminalLink(hostId:String,terminalId:String,scope:kotlinx.coroutines.CoroutineScope,onData:(ByteArray)->Unit,onEvent:(kotlinx.serialization.json.JsonObject)->Unit,onState:(String,String?)->Unit) = repository.terminalLink(hostId,terminalId,scope,onData,onEvent,onState)
    suspend fun git(hostId:String,path:String,op:String,params:Map<String,String>,input:String?=null):Result<String> = try {repository.git(hostId,path,op,params,input)} catch(cancelled:kotlinx.coroutines.CancellationException) {throw cancelled} catch(failure:Exception) {Result.failure(failure)}
    /** Agent-written commit and pull request text for a folder, or null when this host cannot offer it. */
    suspend fun suggestions(hostId:String,path:String): GitSuggestions? {
        val agents=try {repository.suggestionAgents(hostId)} catch(cancelled:kotlinx.coroutines.CancellationException) {throw cancelled} catch(_:Exception) {emptyList()}
        if(agents.isEmpty())return null
        return GitSuggestions(agents,
            {agent->try {Result.success(repository.commitMessage(hostId,path,agent))} catch(cancelled:kotlinx.coroutines.CancellationException) {throw cancelled} catch(failure:Exception) {Result.failure(failure)}},
            {agent,base->try {Result.success(repository.pullRequestText(hostId,path,agent,base))} catch(cancelled:kotlinx.coroutines.CancellationException) {throw cancelled} catch(failure:Exception) {Result.failure(failure)}})
    }
    suspend fun currentBranch(hostId:String,path:String): String? = try {repository.currentBranch(hostId,path)} catch(cancelled:kotlinx.coroutines.CancellationException) {throw cancelled} catch(_:Exception) {null}
    /** The pull request's address, or the readable reason it was not opened. */
    suspend fun openPullRequest(hostId:String,request:PullRequestRequest): Result<String> = try {Result.success(repository.openPullRequest(hostId,request))} catch(cancelled:kotlinx.coroutines.CancellationException) {throw cancelled} catch(failure:Exception) {Result.failure(failure)}
    fun create(hostId:String,agentId:String,workspace:String,onDone:(StoredSession)->Unit)=create(hostId,agentId,workspace,false,onDone)
    fun create(hostId:String,agentId:String,workspace:String,askEverything:Boolean,onDone:(StoredSession)->Unit,yolo:Boolean=false) {
        val key=listOf(hostId,agentId,workspace).joinToString("\u0000")
        if(!creating.add(key))return
        work { try { onDone(repository.create(hostId,agentId,workspace,askEverything=askEverything,yolo=yolo)) } finally { creating.remove(key) } }
    }
    internal fun creationInFlight(hostId:String,agentId:String,workspace:String)=listOf(hostId,agentId,workspace).joinToString("\u0000") in creating
    fun open(stored:StoredSession):Unit = work(onRetry={open(stored)}) { val live=repository.open(stored.hostId,stored.id);ui.update { it.copy(live=live) } }
    fun prompt(live:LiveSession,text:String,uri:String?=null) {val attachments=live.attachments.value;work { repository.prompt(live,text,uri,attachments) }}
    fun addAttachment(live:LiveSession,attachment:PromptAttachment,version:Long) = repository.addAttachment(live,attachment,version)
    fun removeAttachment(live:LiveSession,index:Int) = repository.removeAttachment(live,index)
    fun cancel(live:LiveSession) = work { repository.cancel(live) }
    fun dismissSessionError(live:LiveSession) = repository.dismissSessionError(live)
    fun reconnect(live:LiveSession) = work { repository.reconnect(live) }
    fun takeControl(live:LiveSession) = work { repository.takeControl(live) }
    fun disconnect(live:LiveSession) = work { repository.disconnect(live) }
    fun removeLocalCopy(live:LiveSession) = work {repository.removeLocalCopy(live)}
    fun permission(live:LiveSession,permission:Permission,optionId:String) = work { repository.permission(live,permission,optionId) }
    fun elicitation(live:LiveSession,permission:Permission,action:String,content:JsonObject?) = work { repository.elicitation(live,permission,action,content) }
    fun configure(live:LiveSession,method:String,params:JsonObject) = work { repository.configure(live,method,params) }
    fun draft(stored:StoredSession,text:String) { viewModelScope.launch { repository.draft(stored.hostId,stored.id,text) } }
    fun archive(stored:StoredSession) = work { repository.archive(stored) }
    fun delete(stored:StoredSession,onDone:()->Unit) = work { repository.delete(stored);onDone() }
    fun resume(stored:StoredSession,onDone:(StoredSession)->Unit) = work {
        val info=storedSessionInfo(stored)
        check(info.supportsLoad()) { "This agent does not advertise session loading" }
        check(info.acpSessionId.isNotBlank()) { "Sign in before resuming this session" }
        // Resuming keeps the "Ask for everything" choice the session was started with; the host does not remember it.
        val resumed=repository.create(stored.hostId,info.agentId,info.workspace,info.acpSessionId,askEverything=info.workspaceAccess.manualApproval)
        // The ended record is replaced by the resumed session, so the list never shows both. If the host cannot forget it, it stays.
        if(info.status=="interrupted")try {repository.delete(stored)} catch(cancelled:CancellationException) {throw cancelled} catch(_:Exception) {}
        onDone(resumed)
    }
    val tasks=MutableStateFlow(TasksUiState())
    private var tasksJob:Job?=null
    private fun tasksWork(hostId:String,action:suspend ()->Unit) {
        tasksJob?.cancel()
        tasks.update {if(it.hostId==hostId)it.copy(loading=true,error=null,notice=null) else TasksUiState(hostId,loading=true)}
        tasksJob=viewModelScope.launch {
            try {action()} catch(cancelled:CancellationException) {throw cancelled}
            catch(failure:Exception) {tasks.update {it.copy(error=failure.message ?: "Task list unavailable. Try again.")}}
            finally {tasks.update {it.copy(loading=false)}}
        }
    }
    fun loadTasks(hostId:String,taskId:String?=null) = tasksWork(hostId) {
        val list=repository.tasks(hostId)
        val detail=taskId?.let {repository.task(hostId,it)}
        tasks.update {it.copy(tasks=list,detail=detail)}
    }
    fun mergeTask(hostId:String,id:String) = tasksWork(hostId) {mergeAndRefresh(tasks,{repository.mergeTask(hostId,id)},{repository.tasks(hostId)},{repository.task(hostId,id)})}
    fun discardTask(hostId:String,id:String) = tasksWork(hostId) {discardAndRefresh(tasks,{repository.discardTask(hostId,id)},{repository.tasks(hostId)},{repository.task(hostId,id)})}
    fun cancelTask(hostId:String,id:String) = tasksWork(hostId) {repository.cancelTask(hostId,id);tasks.update {it.copy(tasks=repository.tasks(hostId),detail=repository.task(hostId,id))}}
    fun setTaskSupervision(hostId:String,id:String,paused:Boolean) = tasksWork(hostId) {repository.setTaskSupervision(hostId,id,paused);tasks.update {it.copy(detail=repository.task(hostId,id))}}
    fun forget(hostId:String,onDone:()->Unit) = work { repository.forget(hostId);mcpEdits.remove(hostId)?.close();onDone() }
    fun theme(value:String) = work { repository.settings.theme(value) }
    fun cache(value:Boolean) = work { repository.settings.cache(value);if(!value)repository.clearCache() }
    fun clearError() { ui.update { it.copy(error=null,errorRetry=null) } }
    fun mcp(hostId:String):Unit = work(onRetry={mcp(hostId)}) {ui.update {it.copy(mcpHost=null,mcpServers=emptyList())};val host=repository.mcpHost(hostId);val servers=repository.mcpServers(hostId);ui.update {it.copy(mcpHost=host,mcpServers=servers)}}
    fun saveMcp(hostId:String,servers:List<McpDefinition>,onDone:()->Unit) = work {repository.saveMcpServers(hostId,servers);ui.update {it.copy(mcpServers=servers)};onDone()}
    fun workspaceAccess(hostId:String,path:String,sessionId:String?):Unit = work(onRetry={workspaceAccess(hostId,path,sessionId)}) {
        ui.update {it.copy(accessHost=null,accessPath=null,currentAccess=null)}
        val host=repository.mcpHost(hostId)
        val saved=repository.workspaceAccess(hostId,path)
        val current=sessionId?.let {repository.sessionAccess(hostId,it)}
        ui.update {it.copy(accessHost=host,accessPath=path,savedAccess=saved,currentAccess=current)}
    }
    fun saveWorkspaceAccess(hostId:String,path:String,value:WorkspaceAccess,onDone:()->Unit) = work {repository.saveWorkspaceAccess(hostId,path,value);ui.update {it.copy(savedAccess=value)};onDone()}
}


