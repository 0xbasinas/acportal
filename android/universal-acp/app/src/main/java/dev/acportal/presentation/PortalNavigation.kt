package dev.acportal.presentation

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.selection.selectable
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation3.runtime.*
import androidx.navigation3.ui.NavDisplay
import dev.acportal.storage.StoredSession
import dev.acportal.data.storedSessionInfo
import dev.acportal.protocol.*
import kotlinx.serialization.Serializable

@Serializable data object HostsRoute:NavKey
@Serializable data object SessionsRoute:NavKey
@Serializable data object SessionConnectionRoute:NavKey
@Serializable data object SettingsRoute:NavKey
@Serializable data object UpdateRoute:NavKey
@Serializable data class HostRoute(val id:String):NavKey
@Serializable data class NewSessionRoute(val hostId:String,val agentId:String?=null,val fromSessions:Boolean=false):NavKey
@Serializable data class SessionRoute(val hostId:String,val id:String):NavKey
@Serializable data class SavedSessionRoute(val hostId:String,val id:String):NavKey
@Serializable data class PullRequestRoute(val hostId:String,val id:String):NavKey
@Serializable data class GitRoute(val hostId:String,val path:String):NavKey
@Serializable data class FolderPullRequestRoute(val hostId:String,val path:String):NavKey
@Serializable data object PairRoute:NavKey
@Serializable data class PairHostRoute(val hostId:String):NavKey
@Serializable data class McpRoute(val hostId:String?=null):NavKey
@Serializable data class WorkspaceAccessRoute(val hostId:String?=null,val path:String?=null,val sessionId:String?=null):NavKey
@Serializable data class AddAgentRoute(val hostId:String):NavKey
@Serializable data class TasksRoute(val hostId:String):NavKey
@Serializable data class TaskRoute(val hostId:String,val id:String):NavKey
@Serializable data class AgentRoute(val hostId:String?=null,val agentId:String?=null):NavKey

@Composable fun PortalNavigation(vm:PortalViewModel) {
    val savedUiState by vm.savedUiState.collectAsStateWithLifecycle()
    val saved=savedUiState
    if(saved==null) {Box(Modifier.fillMaxSize(),contentAlignment=Alignment.Center) {CircularProgressIndicator()};return}
    val initialPage=when(saved.mainPage) {"sessions"->SessionsRoute;"settings"->SettingsRoute;else->HostsRoute}
    val stack=rememberNavBackStack(initialPage)
    val hosts by vm.hosts.collectAsStateWithLifecycle()
    val sessions by vm.sessions.collectAsStateWithLifecycle()
    val sessionSummaries by vm.sessionSummaries.collectAsStateWithLifecycle()
    val sharedTitles by vm.sharedTitles.collectAsStateWithLifecycle()
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
                navItems.forEach { (key,icon)->NavigationRailItem(selected=selected==key,onClick={if(stack.lastOrNull()!=key) {stack.clear();stack.add(key)}},icon={Icon(icon,contentDescription=null)},label={Text(label(key))}) }
            }
            Scaffold(modifier=Modifier.weight(1f),bottomBar={ AnimatedVisibility(!wide && showNavigation,enter=expandVertically(tween(PORTAL_PAGE_FADE_MS))+fadeIn(tween(PORTAL_PAGE_FADE_MS)),exit=shrinkVertically(tween(PORTAL_PAGE_FADE_MS))+fadeOut(tween(PORTAL_PAGE_FADE_MS))) { Box(Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal=32.dp,vertical=12.dp),contentAlignment=Alignment.Center) {
                Surface(shape=RoundedCornerShape(32.dp),color=MaterialTheme.colorScheme.surfaceContainer,modifier=Modifier.widthIn(max=360.dp)) {
                    Row(Modifier.padding(7.dp),horizontalArrangement=Arrangement.spacedBy(2.dp)) {
                        navItems.forEach { (key,_)->Surface(shape=RoundedCornerShape(24.dp),color=if(selected==key)MaterialTheme.colorScheme.surfaceContainerHigh else MaterialTheme.colorScheme.surfaceContainer,modifier=Modifier.weight(1f).clip(RoundedCornerShape(24.dp)).semantics {contentDescription="${label(key)} tab"}.selectable(selected==key,role=Role.Tab) {if(stack.lastOrNull()!=key) {stack.clear();stack.add(key)}}) {
                            Box(Modifier.heightIn(min=44.dp).padding(horizontal=6.dp),contentAlignment=Alignment.Center) {Text(label(key),style=MaterialTheme.typography.labelSmall,color=if(selected==key)MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant)}
                        } }
                    }
                }
            } } }) { padding ->
                Column(Modifier.padding(padding).fillMaxSize()) {
                    ui.error?.let { error -> ConnectionError(error,onDismiss=vm::clearError,onRetry=ui.errorRetry) }
                    // The progress bar overlays the page: inserting it above would push the page down mid-transition.
                    Box(Modifier.weight(1f)) {
                    NavDisplay(backStack=stack,onBack=back,modifier=Modifier.fillMaxSize(),
                        sizeTransform=null,
                        transitionSpec={portalPageTransition()},
                        popTransitionSpec={portalPageTransition()},
                        predictivePopTransitionSpec={portalPageTransition()},
                        entryProvider=entryProvider {
                        entry<HostsRoute> { HostsScreen(hosts,onAdd={stack.add(PairRoute)},onHost={stack.add(HostRoute(it.id))},onRefresh=vm::refresh) }
                        entry<PairRoute> { PairScreen(ui.loading,onBack=back,onPair={address,code,label->vm.pair(address,code,label,back)}) }
                        entry<PairHostRoute> {key->
                            val host=hosts.firstOrNull {it.id==key.hostId}
                            if(host!=null)PairScreen(ui.loading,onBack=back,onPair={address,code,label->vm.pair(address,code,label,back)},initialAddress=host.address,initialLabel=host.label)
                            else EmptyState("Connection unavailable","Return to Connections to add this host again.")
                        }
                        entry<HostRoute> { key ->
                            LaunchedEffect(key.id) { vm.details(key.id) }
                            HostDetailsScreen(ui.details?.takeIf { it.host.id==key.id },onBack=back,onNew={stack.add(NewSessionRoute(key.id))},onSession={info->sessions.find { it.hostId==key.id && it.id==info.id }?.let(openSession)},onRefresh={vm.details(key.id)},onForget={vm.forget(key.id,back)},onAgent={stack.add(AgentRoute(key.id,it))},loading=ui.loading,onTasks={stack.add(TasksRoute(key.id))})
                        }
                        entry<TasksRoute> {key->
                            val tasks by vm.tasks.collectAsStateWithLifecycle()
                            LaunchedEffect(key.hostId) {vm.loadTasks(key.hostId)}
                            TasksScreen(tasks.takeIf {it.hostId==key.hostId} ?: TasksUiState(key.hostId,loading=true),back,{vm.loadTasks(key.hostId)},{stack.add(TaskRoute(key.hostId,it))})
                        }
                        entry<TaskRoute> {key->
                            val tasks by vm.tasks.collectAsStateWithLifecycle()
                            LaunchedEffect(key.hostId,key.id) {vm.loadTasks(key.hostId,key.id)}
                            TaskDetailScreen(tasks.takeIf {it.hostId==key.hostId} ?: TasksUiState(key.hostId,loading=true),key.id,back,{vm.loadTasks(key.hostId,key.id)},{vm.cancelTask(key.hostId,key.id)},{vm.setTaskSupervision(key.hostId,key.id,it)},onMerge={vm.mergeTask(key.hostId,key.id)},onDiscard={vm.discardTask(key.hostId,key.id)},
                                onSession={session->sessions.find {it.hostId==key.hostId && it.id==session}?.let(openSession)})
                        }
                        entry<NewSessionRoute> { key ->
                            var askEverything by rememberSaveable(key) {mutableStateOf(false)}
                            // "supervised", "ask-all" or "yolo". Never remembered: each new session chooses again.
                            var permissionId by rememberSaveable(key) {mutableStateOf("supervised")}
                            val permissionYolo=permissionId==PRESET_YOLO
                            val permissionAsk=askEverything || permissionId==PRESET_ASK_ALL
                            LaunchedEffect(key.hostId) { if(key.fromSessions || ui.details?.host?.id!=key.hostId)vm.details(key.hostId) }
                            NewSessionScreen(ui.details?.takeIf {it.host.id==key.hostId},ui.workspaceBrowse.folders,ui.loading,onBack=back,onMcp={stack.add(McpRoute(key.hostId))},onAccess={path->stack.add(WorkspaceAccessRoute(key.hostId,path))},onBrowse=vm::browse,onCreate={agent,path->vm.create(key.hostId,agent,path,permissionAsk,{ stored->if(stack.lastOrNull()==key) {stack.removeLastOrNull();if(stack.lastOrNull()==SessionConnectionRoute)stack.removeLastOrNull();openSession(stored)} },permissionYolo)},initialAgentId=key.agentId,browseError=ui.workspaceBrowse.error,browseLoading=ui.workspaceBrowse.loading,onDismissBrowse=vm::cancelBrowse,onReload={vm.details(key.hostId)},onChangeConnection=if(key.fromSessions)back else null,folderClient=ui.details?.takeIf {it.host.id==key.hostId}?.let {vm.folderClient(it.host)},onAddAgent={stack.add(AddAgentRoute(key.hostId))},askEverything=askEverything,onAskEverything={askEverything=it},permissionId=permissionId,onPermission={permissionId=it},loadPresets={agent->vm.presets(key.hostId,agent)},loadEarlier=if(HOST_FEATURE_AGENT_SESSION_LIST in (ui.details?.takeIf {it.host.id==key.hostId}?.features ?: emptyList())) {agent,path->vm.earlierSessions(key.hostId,agent,path)} else null,onOpenEarlier={agent,path,id->vm.createEarlier(key.hostId,agent,path,id,permissionAsk,{ stored->if(stack.lastOrNull()==key) {stack.removeLastOrNull();if(stack.lastOrNull()==SessionConnectionRoute)stack.removeLastOrNull();openSession(stored)} },permissionYolo)})
                        }
                        entry<SessionConnectionRoute> { SessionConnectionScreen(hosts,onBack=back,onPair={stack.add(PairRoute)},onChoose={vm.clearError();stack.add(NewSessionRoute(it,fromSessions=true))}) }
                        entry<SessionsRoute> { val allTurnTimes by vm.turnTimes.collectAsStateWithLifecycle();val projects by vm.hostProjects.collectAsStateWithLifecycle();SessionsScreen(sessions,hosts,turnTimes=allTurnTimes,projects=projects,onGit={host,path->vm.details(host);stack.add(GitRoute(host,path))},onClearEnded={list->list.forEach {vm.delete(it,{})}},onOpen=openSession,onArchive=vm::archive,onResume={vm.resume(it,openSession)},onDelete={vm.delete(it,{})},onRefresh=vm::refresh,onNew={vm.clearError();stack.add(SessionConnectionRoute)},liveActivities=liveActivities,initialUiState=saved,onFiltersChanged=vm::sessionFilters,summaries=sessionSummaries,sharedTitles=sharedTitles) }
                        entry<SessionRoute> { key ->
                            val conversationState=rememberConversationUiState(key.hostId+"/"+key.id)
                            val allTurnTimes by vm.turnTimes.collectAsStateWithLifecycle()
                            val stored=sessions.find { it.hostId==key.hostId && it.id==key.id }
                            val info=stored?.let(::storedSessionInfo)
                            LaunchedEffect(key.hostId,key.id,stored!=null,info?.status) { if(info?.status!="interrupted")stored?.let(vm::open) }
                            val live=ui.live?.takeIf { it.host.id==key.hostId && it.info.id==key.id }
                            val pullRequestAction:(()->Unit)?=if(HOST_FEATURE_PULL_REQUESTS in (ui.details?.takeIf {it.host.id==key.hostId}?.features ?: emptyList())) ({stack.add(PullRequestRoute(key.hostId,key.id))}) else null
                            if(info?.status=="interrupted") InterruptedSessionScreen(info,ui.loading,back,{vm.resume(stored,openSession)},{vm.create(stored.hostId,info.agentId,info.workspace,openSession)},onViewSaved={stack.add(SavedSessionRoute(key.hostId,key.id))})
                            else if(live!=null && stored!=null) SessionScreen(live,stored,onBack=back,onPrompt={text,uri->vm.prompt(live,text,uri)},onCancel={vm.cancel(live)},onPermission={permission,option->vm.permission(live,permission,option)},onElicitation={permission,action,content->vm.elicitation(live,permission,action,content)},onConfigure={method,params->vm.configure(live,method,params)},onDraft={vm.draft(stored,it)},onReconnect={vm.reconnect(live)},onTakeControl={vm.takeControl(live)},onDisconnect={vm.disconnect(live)},onMcp={stack.add(McpRoute(key.hostId))},onAccess={stack.add(WorkspaceAccessRoute(key.hostId,live.info.workspace,key.id))},onPullRequest=pullRequestAction,onRemoveLocal={vm.removeLocalCopy(live)},turnTimes=allTurnTimes["${key.hostId}/${key.id}"].orEmpty(),onAddAttachment={attachment,version->vm.addAttachment(live,attachment,version)},onRemoveAttachment={vm.removeAttachment(live,it)},onDismissError={vm.dismissSessionError(live)},onSessions={stack.clear();stack.add(SessionsRoute)},onPairHost={stack.clear();stack.add(HostsRoute);stack.add(PairHostRoute(key.hostId))},conversationState=conversationState)
                            else if(stored!=null && info!=null)SavedSessionScreen(info,stored,hosts.firstOrNull {it.id==key.hostId},ui.loading,back,{vm.draft(stored,it)},{vm.open(stored)},conversationState=conversationState)
                            else Column(Modifier.fillMaxSize()) {ScreenHeader("Session unavailable",onBack=back);EmptyState("This session is no longer saved","Return to Sessions to choose another conversation.")}
                        }
                        entry<GitRoute> {key->
                            val host=hosts.firstOrNull {it.id==key.hostId}
                            val hasPullRequests=HOST_FEATURE_PULL_REQUESTS in (ui.details?.takeIf {it.host.id==key.hostId}?.features ?: emptyList())
                            var suggestions by remember(key) {mutableStateOf<GitSuggestions?>(null)}
                            LaunchedEffect(key) {suggestions=vm.suggestions(key.hostId,key.path)}
                            GitScreen(key.path,host?.label,GitRun {op,params,input->vm.git(key.hostId,key.path,op,params,input)},if(hasPullRequests) ({stack.add(FolderPullRequestRoute(key.hostId,key.path))}) else null,back,suggestions)
                        }
                        entry<FolderPullRequestRoute> {key->
                            var suggestions by remember(key) {mutableStateOf<GitSuggestions?>(null)}
                            LaunchedEffect(key) {suggestions=vm.suggestions(key.hostId,key.path)}
                            PullRequestScreen(hosts.firstOrNull {it.id==key.hostId}?.label,key.path,{vm.currentBranch(key.hostId,key.path)},{request->vm.openPullRequest(key.hostId,request)},back,suggestions)
                        }
                        entry<PullRequestRoute> {key->
                            val stored=sessions.find {it.hostId==key.hostId && it.id==key.id}
                            val info=stored?.let(::storedSessionInfo)
                            if(info==null)Column(Modifier.fillMaxSize()) {ScreenHeader("Open pull request",onBack=back);EmptyState("This session is no longer saved","Return to Sessions to choose another conversation.")}
                            else {
                                var suggestions by remember(key) {mutableStateOf<GitSuggestions?>(null)}
                                LaunchedEffect(key) {suggestions=vm.suggestions(key.hostId,info.workspace)}
                                PullRequestScreen(hosts.firstOrNull {it.id==key.hostId}?.label,info.workspace,{vm.currentBranch(key.hostId,info.workspace)},{request->vm.openPullRequest(key.hostId,request)},back,suggestions)
                            }
                        }
                        entry<SavedSessionRoute> {key->
                            val stored=sessions.firstOrNull {it.hostId==key.hostId && it.id==key.id}
                            val info=stored?.let(::storedSessionInfo)
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
                        entry<AddAgentRoute> {key->
                            val host=hosts.firstOrNull {it.id==key.hostId}
                            if(host==null)EmptyState("Connection unavailable","Return to Connections to add this host again.")
                            else AddAgentScreen(vm.agentClient(host),host.label,onBack=back,onSaved={vm.details(key.hostId);back()})
                        }
                        entry<AgentRoute> {key->
                            val id=key.hostId
                            if(id==null)ConnectionsPickerScreen(hosts,back,{stack.add(AgentRoute(it))},"Agents")
                            else {
                                // Reuse fresh in-memory discovery on detail/Back navigation.
                                // Clearing it here briefly inserts the saved-list notice into
                                // large-font lists and changes their restored scroll anchor.
                                LaunchedEffect(id) {if(ui.details?.host?.id!=id || hosts.none {it.id==id && it.online})vm.details(id)}
                                val fresh=ui.details?.takeIf {it.host.id==id && hosts.any {host->host.id==id && host.online}}
                                val saved=agentCatalogs.firstOrNull {it.hostId==id}
                                val savedHost=hosts.firstOrNull {it.id==id}
                                val details=fresh ?: if(saved!=null && savedHost!=null)runCatching {dev.acportal.data.HostDetails(savedHost.copy(online=false),WireJson.decodeFromString<List<AgentInfo>>(saved.metadata),emptyList(),emptyList(),emptyList())}.getOrNull() else null
                                val savedAt=if(fresh==null)saved?.discoveredAt else null
                                if(details==null)Column(Modifier.fillMaxSize()) {
                                    ScreenHeader("Agents",onBack=back)
                                    if(ui.loading)EmptyState("Loading agents","Reading this connection's agent registry.")
                                    else {
                                        Box(Modifier.weight(1f)) {EmptyState("Agents unavailable","Discovery could not be completed. Reload or return to choose another connection.")}
                                        TextButton({vm.details(id)},Modifier.fillMaxWidth().padding(24.dp)) {Text("Reload agents")}
                                    }
                                }
                                else if(key.agentId==null)RegistryAgentsScreen(details,back,{vm.details(id)},{stack.add(AgentRoute(id,it))},savedAt=savedAt,onAddAgent=if(fresh!=null){{stack.add(AddAgentRoute(id))}} else null)
                                else {
                                    val agent=details.agents.firstOrNull {it.id==key.agentId}
                                    if(agent==null)Column(Modifier.fillMaxSize()) {ScreenHeader("Agent details",onBack=back);EmptyState("Agent unavailable","This agent is no longer in the host registry.")}
                                    else RegistryAgentDetailsScreen(agent,details.host.label,ui.loading,back,{vm.details(id)},{stack.add(NewSessionRoute(id,agent.id))},savedAt=savedAt,onRemove=if(fresh!=null){{vm.removeAgent(details.host,agent.id) {vm.details(id);back()}}} else null)
                                }
                            }
                        }
                        entry<SettingsRoute> { SettingsScreen(settings,onTheme=vm::theme,onCache=vm::cache,onConnections={stack.add(HostsRoute)},onMcp={stack.add(McpRoute())},onAccess={stack.add(WorkspaceAccessRoute())},onAgents={stack.add(AgentRoute())},onUpdate={stack.add(UpdateRoute)},updateValue="Build ${(LocalContext.current.applicationContext as dev.acportal.PortalApplication).updates.installed.build}") }
                        entry<UpdateRoute> {
                            val context=LocalContext.current
                            val updates=(context.applicationContext as dev.acportal.PortalApplication).updates
                            val update by updates.state.collectAsStateWithLifecycle()
                            // Look for a newer build whenever the screen opens, unless a download or install is under way.
                            LaunchedEffect(Unit) {if(updates.state.value !is dev.acportal.data.UpdateState.Downloading && updates.state.value !is dev.acportal.data.UpdateState.Installing)updates.check()}
                            val owner=androidx.lifecycle.compose.LocalLifecycleOwner.current
                            DisposableEffect(owner) {
                                val observer=androidx.lifecycle.LifecycleEventObserver {_,event->if(event==androidx.lifecycle.Lifecycle.Event.ON_RESUME)updates.permissionMayHaveChanged()}
                                owner.lifecycle.addObserver(observer);onDispose {owner.lifecycle.removeObserver(observer)}
                            }
                            val channel by updates.channel.collectAsStateWithLifecycle()
                            UpdateScreen(update,updates.installed,back,updates::check,updates::install,updates::cancel,{context.startActivity(updates.permissionIntent())},channel,updates::selectChannel,updates.versions.collectAsStateWithLifecycle().value)
                        }
                    })
                    if(ui.loading) LinearProgressIndicator(Modifier.fillMaxWidth().align(Alignment.TopCenter))
                    }
                }
            }
        }
    }
}

