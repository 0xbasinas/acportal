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
class PortalViewModel(val repository:PortalRepository):ViewModel() {
    val hosts=repository.hosts.stateIn(viewModelScope,SharingStarted.WhileSubscribed(5000),emptyList())
    val sessions=repository.sessions.stateIn(viewModelScope,SharingStarted.WhileSubscribed(5000),emptyList())
    val liveActivities=repository.liveActivities
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
    init { refresh() }
    private fun work(onRetry:(()->Unit)?=null,action:suspend ()->Unit) { viewModelScope.launch { ui.update { it.copy(loading=true,error=null,errorRetry=null) };try { action() } catch (cancelled:CancellationException) { throw cancelled } catch (failure:Exception) { ui.update { it.copy(error=failure.message ?: "Something went wrong. Try again.",errorRetry=onRetry) } } finally { ui.update { it.copy(loading=false) } } } }
    fun refresh() = work { repository.refreshHosts() }
    fun details(id:String) {cancelBrowse();work(onRetry={details(id)}) { ui.update { it.copy(details=null) };val details=repository.details(id);ui.update { it.copy(details=details) } }}
    fun pair(address:String,code:String,label:String,onDone:()->Unit) = work { repository.pair(address,code,label);repository.refreshHosts();onDone() }
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
    fun create(hostId:String,agentId:String,workspace:String,onDone:(StoredSession)->Unit) = work { onDone(repository.create(hostId,agentId,workspace)) }
    fun open(stored:StoredSession):Unit = work(onRetry={open(stored)}) { val live=repository.open(stored.hostId,stored.id);ui.update { it.copy(live=live) } }
    fun prompt(live:LiveSession,text:String,uri:String?=null) {val attachments=live.attachments.value;work { repository.prompt(live,text,uri,attachments) }}
    fun addAttachment(live:LiveSession,attachment:PromptAttachment,version:Long) = repository.addAttachment(live,attachment,version)
    fun removeAttachment(live:LiveSession,index:Int) = repository.removeAttachment(live,index)
    fun cancel(live:LiveSession) = work { repository.cancel(live) }
    fun dismissSessionError(live:LiveSession) = repository.dismissSessionError(live)
    fun reconnect(live:LiveSession) = work { repository.reconnect(live) }
    fun disconnect(live:LiveSession) = work { repository.disconnect(live) }
    fun removeLocalCopy(live:LiveSession) = work {repository.removeLocalCopy(live)}
    fun permission(live:LiveSession,permission:Permission,optionId:String) = work { repository.permission(live,permission,optionId) }
    fun configure(live:LiveSession,method:String,params:JsonObject) = work { repository.configure(live,method,params) }
    fun draft(stored:StoredSession,text:String) { viewModelScope.launch { repository.draft(stored.hostId,stored.id,text) } }
    fun archive(stored:StoredSession) = work { repository.archive(stored) }
    fun delete(stored:StoredSession,onDone:()->Unit) = work { repository.delete(stored);onDone() }
    fun resume(stored:StoredSession,onDone:(StoredSession)->Unit) = work {
        val info=WireJson.decodeFromString<SessionInfo>(stored.metadata)
        check(info.supportsLoad()) { "This agent does not advertise session loading" }
        check(info.acpSessionId.isNotBlank()) { "Sign in before resuming this session" }
        onDone(repository.create(stored.hostId,info.agentId,info.workspace,info.acpSessionId))
    }
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


