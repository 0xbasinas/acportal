package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.selection.selectable
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation3.runtime.*
import androidx.navigation3.ui.NavDisplay
import dev.acportal.storage.StoredSession
import dev.acportal.protocol.*
import kotlinx.serialization.Serializable

@Serializable data object HostsRoute:NavKey
@Serializable data object SessionsRoute:NavKey
@Serializable data object SettingsRoute:NavKey
@Serializable data class HostRoute(val id:String):NavKey
@Serializable data class NewSessionRoute(val hostId:String,val agentId:String?=null):NavKey
@Serializable data class SessionRoute(val hostId:String,val id:String):NavKey
@Serializable data class SavedSessionRoute(val hostId:String,val id:String):NavKey
@Serializable data object PairRoute:NavKey
@Serializable data class PairHostRoute(val hostId:String):NavKey
@Serializable data class McpRoute(val hostId:String?=null):NavKey
@Serializable data class WorkspaceAccessRoute(val hostId:String?=null,val path:String?=null,val sessionId:String?=null):NavKey
@Serializable data class AgentRoute(val hostId:String?=null,val agentId:String?=null):NavKey

@Composable fun PortalNavigation(vm:PortalViewModel) {
    val savedUiState by vm.savedUiState.collectAsStateWithLifecycle()
    val saved=savedUiState
    if(saved==null) {Box(Modifier.fillMaxSize(),contentAlignment=Alignment.Center) {CircularProgressIndicator()};return}
    val initialPage=when(saved.mainPage) {"sessions"->SessionsRoute;"settings"->SettingsRoute;else->HostsRoute}
    val stack=rememberNavBackStack(initialPage)
    val hosts by vm.hosts.collectAsStateWithLifecycle()
    val sessions by vm.sessions.collectAsStateWithLifecycle()
    val liveActivities by vm.liveActivities.collectAsStateWithLifecycle()
    val agentCatalogs by vm.agentCatalogs.collectAsStateWithLifecycle()
    val settings by vm.settings.collectAsStateWithLifecycle()
    val ui by vm.ui.collectAsStateWithLifecycle()
    val route=stack.lastOrNull() ?: HostsRoute
    LaunchedEffect(route) {when(route) {HostsRoute->vm.mainPage("connections");SessionsRoute->vm.mainPage("sessions");SettingsRoute->vm.mainPage("settings");else->Unit}}
    val back={ stack.removeLastOrNull();Unit }
    val openSession:(StoredSession)->Unit={ stored -> stack.add(SessionRoute(stored.hostId,stored.id)) }
    val navItems=listOf(HostsRoute to Icons.Outlined.Dns,SessionsRoute to Icons.Outlined.Terminal,SettingsRoute to Icons.Outlined.Settings)
    fun label(key:NavKey)=when(key) {HostsRoute->"Connections";SessionsRoute->"Sessions";else->"Settings"}
    val selected=when(route) { is SessionRoute,SessionsRoute->SessionsRoute;SettingsRoute->SettingsRoute;else->HostsRoute }
    val showNavigation=route is HostsRoute || route is SessionsRoute || route is SettingsRoute
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val wide=maxWidth>=700.dp
        Row(Modifier.fillMaxSize()) {
            if(wide && showNavigation) NavigationRail {
                Spacer(Modifier.height(24.dp))
                navItems.forEach { (key,icon)->NavigationRailItem(selected=selected==key,onClick={stack.clear();stack.add(key)},icon={Icon(icon,contentDescription=null)},label={Text(label(key))}) }
            }
            Scaffold(modifier=Modifier.weight(1f),bottomBar={ if(!wide && showNavigation) Box(Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal=32.dp,vertical=12.dp),contentAlignment=Alignment.Center) {
                Surface(shape=RoundedCornerShape(32.dp),color=MaterialTheme.colorScheme.surfaceContainer,modifier=Modifier.widthIn(max=360.dp)) {
                    Row(Modifier.padding(7.dp),horizontalArrangement=Arrangement.spacedBy(2.dp)) {
                        navItems.forEach { (key,_)->Surface(shape=RoundedCornerShape(24.dp),color=if(selected==key)MaterialTheme.colorScheme.surfaceContainerHigh else MaterialTheme.colorScheme.surfaceContainer,modifier=Modifier.weight(1f).clip(RoundedCornerShape(24.dp)).semantics {contentDescription="${label(key)} tab"}.selectable(selected==key,role=Role.Tab) {stack.clear();stack.add(key)}) {
                            Box(Modifier.heightIn(min=44.dp).padding(horizontal=6.dp),contentAlignment=Alignment.Center) {Text(label(key),style=MaterialTheme.typography.labelSmall,color=if(selected==key)MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant)}
                        } }
                    }
                }
            } }) { padding ->
                Column(Modifier.padding(padding).fillMaxSize()) {
                    if(ui.loading) LinearProgressIndicator(Modifier.fillMaxWidth())
                    ui.error?.let { error -> ConnectionError(error,onDismiss=vm::clearError,onRetry=ui.errorRetry) }
                    NavDisplay(backStack=stack,onBack=back,modifier=Modifier.weight(1f),entryProvider=entryProvider {
                        entry<HostsRoute> { HostsScreen(hosts,onAdd={stack.add(PairRoute)},onHost={stack.add(HostRoute(it.id))},onRefresh=vm::refresh) }
                        entry<PairRoute> { PairScreen(ui.loading,onBack=back,onPair={address,code,label->vm.pair(address,code,label,back)}) }
                        entry<PairHostRoute> {key->
                            val host=hosts.firstOrNull {it.id==key.hostId}
                            if(host!=null)PairScreen(ui.loading,onBack=back,onPair={address,code,label->vm.pair(address,code,label,back)},initialAddress=host.address,initialLabel=host.label)
                            else EmptyState("Connection unavailable","Return to Connections to add this host again.")
                        }
                        entry<HostRoute> { key ->
                            LaunchedEffect(key.id) { vm.details(key.id) }
                            HostDetailsScreen(ui.details?.takeIf { it.host.id==key.id },onBack=back,onNew={stack.add(NewSessionRoute(key.id))},onSession={info->sessions.find { it.hostId==key.id && it.id==info.id }?.let(openSession)},onRefresh={vm.details(key.id)},onForget={vm.forget(key.id,back)},onAgent={stack.add(AgentRoute(key.id,it))})
                        }
                        entry<NewSessionRoute> { key ->
                            LaunchedEffect(key.hostId) { if(ui.details?.host?.id!=key.hostId)vm.details(key.hostId) }
                            NewSessionScreen(ui.details?.takeIf {it.host.id==key.hostId},ui.workspaceBrowse.folders,ui.loading,onBack=back,onMcp={stack.add(McpRoute(key.hostId))},onAccess={path->stack.add(WorkspaceAccessRoute(key.hostId,path))},onBrowse=vm::browse,onCreate={agent,path->vm.create(key.hostId,agent,path) { stored->stack.removeLastOrNull();openSession(stored) }},initialAgentId=key.agentId,browseError=ui.workspaceBrowse.error,browseLoading=ui.workspaceBrowse.loading,onDismissBrowse=vm::cancelBrowse)
                        }
                        entry<SessionsRoute> { SessionsScreen(sessions,hosts,onOpen=openSession,onArchive=vm::archive,onResume={vm.resume(it,openSession)},onDelete={vm.delete(it,{})},onRefresh=vm::refresh,onNew={stack.add(HostsRoute)},liveActivities=liveActivities,initialUiState=saved,onFiltersChanged=vm::sessionFilters) }
                        entry<SessionRoute> { key ->
                            val stored=sessions.find { it.hostId==key.hostId && it.id==key.id }
                            val info=stored?.let {WireJson.decodeFromString<SessionInfo>(it.metadata)}
                            LaunchedEffect(key.hostId,key.id,stored!=null,info?.status) { if(info?.status!="interrupted")stored?.let(vm::open) }
                            val live=ui.live?.takeIf { it.host.id==key.hostId && it.info.id==key.id }
                            if(info?.status=="interrupted") InterruptedSessionScreen(info,ui.loading,back,{vm.resume(stored,openSession)},{vm.create(stored.hostId,info.agentId,info.workspace,openSession)},onViewSaved={stack.add(SavedSessionRoute(key.hostId,key.id))})
                            else if(live!=null && stored!=null) SessionScreen(live,stored,onBack=back,onPrompt={text,uri->vm.prompt(live,text,uri)},onCancel={vm.cancel(live)},onPermission={permission,option->vm.permission(live,permission,option)},onElicitation={permission,action,content->vm.elicitation(live,permission,action,content)},onConfigure={method,params->vm.configure(live,method,params)},onDraft={vm.draft(stored,it)},onReconnect={vm.reconnect(live)},onDisconnect={vm.disconnect(live)},onMcp={stack.add(McpRoute(key.hostId))},onAccess={stack.add(WorkspaceAccessRoute(key.hostId,live.info.workspace,key.id))},onRemoveLocal={vm.removeLocalCopy(live)},onAddAttachment={attachment,version->vm.addAttachment(live,attachment,version)},onRemoveAttachment={vm.removeAttachment(live,it)},onDismissError={vm.dismissSessionError(live)},onSessions={stack.clear();stack.add(SessionsRoute)},onPairHost={stack.clear();stack.add(HostsRoute);stack.add(PairHostRoute(key.hostId))})
                            else if(stored!=null && info!=null)SavedSessionScreen(info,stored,hosts.firstOrNull {it.id==key.hostId},ui.loading,back,{vm.draft(stored,it)},{vm.open(stored)})
                            else Column(Modifier.fillMaxSize()) {ScreenHeader("Session unavailable",onBack=back);EmptyState("This session is no longer saved","Return to Sessions to choose another conversation.")}
                        }
                        entry<SavedSessionRoute> {key->
                            val stored=sessions.firstOrNull {it.hostId==key.hostId && it.id==key.id}
                            val info=stored?.let {WireJson.decodeFromString<SessionInfo>(it.metadata)}
                            if(stored!=null && info!=null)SavedSessionScreen(info,stored,hosts.firstOrNull {it.id==key.hostId},false,back,{vm.draft(stored,it)},back,onRetryLabel="Session actions")
                            else Column(Modifier.fillMaxSize()) {ScreenHeader("Session unavailable",onBack=back);EmptyState("This session is no longer saved","Return to Sessions to choose another conversation.")}
                        }
                        entry<McpRoute> {key->
                            val id=key.hostId
                            if(id==null)ConnectionsPickerScreen(hosts,back,{stack.add(McpRoute(it))})
                            else {
                                LaunchedEffect(id) {vm.mcp(id)}
                                val host=ui.mcpHost?.takeIf {it.id==id}
                                if(host!=null)McpServersScreen(host.label,ui.mcpServers,ui.loading,back,{servers,done->vm.saveMcp(id,servers,done)},editState=vm.mcpEdit(id))
                                else Column(Modifier.fillMaxSize()) {
                                    ScreenHeader("MCP servers",onBack=back)
                                    if(ui.loading)EmptyState("Loading servers","Reading this connection's saved definitions.")
                                    else {
                                        Box(Modifier.weight(1f)) {EmptyState("Servers unavailable","Saved definitions have not been changed. Reload or return to choose another connection.")}
                                        TextButton({vm.mcp(id)},Modifier.fillMaxWidth().padding(24.dp)) {Text("Reload servers")}
                                    }
                                }
                            }
                        }
                        entry<WorkspaceAccessRoute> {key->
                            val id=key.hostId;val path=key.path
                            if(id==null)ConnectionsPickerScreen(hosts,back,{stack.add(WorkspaceAccessRoute(it))},"Workspace access")
                            else if(path==null) {
                                LaunchedEffect(id) {vm.details(id)}
                                WorkspaceAccessListScreen(ui.details?.takeIf {it.host.id==id},ui.workspaceBrowse.folders,ui.loading,back,{folder->ui.details?.host?.let {vm.browse(it,folder)}},{folder->stack.add(WorkspaceAccessRoute(id,folder))},browseError=ui.workspaceBrowse.error,browseLoading=ui.workspaceBrowse.loading,onDismissBrowse=vm::cancelBrowse,onReload={vm.details(id)})
                            } else {
                                LaunchedEffect(id,path,key.sessionId) {vm.workspaceAccess(id,path,key.sessionId)}
                                val host=ui.accessHost?.takeIf {it.id==id && ui.accessPath==path}
                                if(host!=null)WorkspaceAccessScreen(host.label,path,ui.savedAccess,ui.currentAccess,ui.loading,back,{value->vm.saveWorkspaceAccess(id,path,value,back)})
                                else Column(Modifier.fillMaxSize()) {
                                    ScreenHeader("Workspace access",onBack=back)
                                    if(ui.loading)EmptyState("Loading access","Reading the workspace's saved access settings.")
                                    else {
                                        Box(Modifier.weight(1f)) {EmptyState("Access unavailable","Saved access has not been changed. Reload or return to choose another workspace.")}
                                        TextButton({vm.workspaceAccess(id,path,key.sessionId)},Modifier.fillMaxWidth().padding(24.dp)) {Text("Reload access")}
                                    }
                                }
                            }
                        }
                        entry<AgentRoute> {key->
                            val id=key.hostId
                            if(id==null)ConnectionsPickerScreen(hosts,back,{stack.add(AgentRoute(it))},"Agents")
                            else {
                                LaunchedEffect(id) {vm.details(id)}
                                val fresh=ui.details?.takeIf {it.host.id==id && hosts.any {host->host.id==id && host.online}}
                                val saved=agentCatalogs.firstOrNull {it.hostId==id}
                                val savedHost=hosts.firstOrNull {it.id==id}
                                val details=fresh ?: if(saved!=null && savedHost!=null)runCatching {dev.acportal.data.HostDetails(savedHost.copy(online=false),WireJson.decodeFromString<List<AgentInfo>>(saved.metadata),emptyList(),emptyList(),emptyList())}.getOrNull() else null
                                val savedAt=if(fresh==null)saved?.discoveredAt else null
                                if(details==null)EmptyState("Loading agents","Reading this connection's agent registry.")
                                else if(key.agentId==null)RegistryAgentsScreen(details,back,{vm.details(id)},{stack.add(AgentRoute(id,it))},savedAt=savedAt)
                                else {
                                    val agent=details.agents.firstOrNull {it.id==key.agentId}
                                    if(agent==null)Column(Modifier.fillMaxSize()) {ScreenHeader("Agent details",onBack=back);EmptyState("Agent unavailable","This agent is no longer in the host registry.")}
                                    else RegistryAgentDetailsScreen(agent,details.host.label,ui.loading,back,{vm.details(id)},{stack.add(NewSessionRoute(id,agent.id))},savedAt=savedAt)
                                }
                            }
                        }
                        entry<SettingsRoute> { SettingsScreen(settings,onTheme=vm::theme,onCache=vm::cache,onConnections={stack.add(HostsRoute)},onMcp={stack.add(McpRoute())},onAccess={stack.add(WorkspaceAccessRoute())},onAgents={stack.add(AgentRoute())}) }
                    })
                }
            }
        }
    }
}

