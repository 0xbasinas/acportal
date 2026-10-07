package dev.acportal.presentation

import androidx.compose.foundation.clickable
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.Alignment
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.tooling.preview.Preview
import dev.acportal.data.HostDetails
import dev.acportal.data.LiveSessionActivity
import dev.acportal.data.recentWorkspaceChoices
import dev.acportal.data.workspaceInCurrentRoots
import kotlinx.coroutines.delay
import dev.acportal.protocol.*
import dev.acportal.storage.*
import kotlinx.serialization.json.*

@Composable fun ScreenHeader(title:String,subtitle:String?=null,onBack:(()->Unit)?=null,action:(@Composable ()->Unit)?=null) {
    Row(Modifier.fillMaxWidth().padding(start=if(onBack==null)24.dp else 12.dp,end=16.dp,top=20.dp,bottom=24.dp),verticalAlignment=Alignment.CenterVertically) {
        onBack?.let { IconButton(onClick=it) { Icon(Icons.AutoMirrored.Outlined.ArrowBack,"Back",Modifier.size(22.dp)) };Spacer(Modifier.width(4.dp)) }
        Column(Modifier.weight(1f)) { Text(title,style=MaterialTheme.typography.headlineSmall,maxLines=1,overflow=TextOverflow.Ellipsis);subtitle?.let { Text(it,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant,maxLines=1,overflow=TextOverflow.Ellipsis) } }
        action?.invoke()
    }
}
@Composable fun SectionLabel(text:String) { Text(text,Modifier.padding(start=24.dp,top=24.dp,bottom=8.dp),style=MaterialTheme.typography.labelLarge,color=MaterialTheme.colorScheme.onSurfaceVariant) }
@Composable fun EmptyState(title:String,description:String) {
    Box(Modifier.fillMaxSize().padding(32.dp),contentAlignment=Alignment.Center) { Column(horizontalAlignment=Alignment.CenterHorizontally,verticalArrangement=Arrangement.spacedBy(12.dp)) {
        Icon(Icons.Outlined.Code,null,Modifier.size(36.dp),tint=MaterialTheme.colorScheme.primary)
        Text(title,style=MaterialTheme.typography.titleLarge)
        Text(description,style=MaterialTheme.typography.bodyMedium,color=MaterialTheme.colorScheme.onSurfaceVariant)
    } }
}
@Composable fun ConnectionError(message:String,onDismiss:()->Unit,onRetry:(()->Unit)?=null) {
    Surface(Modifier.fillMaxWidth().padding(horizontal=16.dp,vertical=8.dp),shape=androidx.compose.foundation.shape.RoundedCornerShape(12.dp),color=MaterialTheme.colorScheme.surfaceContainer) { Row(Modifier.padding(start=16.dp,end=4.dp,top=4.dp,bottom=4.dp),verticalAlignment=Alignment.CenterVertically) {
        Text(message,Modifier.weight(1f),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
        onRetry?.let {TextButton(it) {Text("Retry")}}
        IconButton(onClick=onDismiss) { Icon(Icons.Outlined.Close,"Dismiss error") }
    } }
}
@Composable fun HostsScreen(hosts:List<HostProfile>,onAdd:()->Unit,onHost:(HostProfile)->Unit,onRefresh:()->Unit) {
    Column(Modifier.fillMaxSize()) {
        ScreenHeader("Connections",action={IconButton(onRefresh) { Icon(Icons.Outlined.Refresh,"Refresh connections",Modifier.size(20.dp),tint=MaterialTheme.colorScheme.onSurfaceVariant) }})
        LazyColumn(Modifier.fillMaxWidth(),contentPadding=PaddingValues(bottom=24.dp)) {
            items(hosts,key={it.id}) {host->Row(Modifier.fillMaxWidth().clickable {onHost(host)}.padding(horizontal=24.dp,vertical=20.dp),verticalAlignment=Alignment.CenterVertically) {
                Column(Modifier.weight(1f).padding(end=12.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {
                    Text(host.label,style=MaterialTheme.typography.titleMedium,maxLines=2,overflow=TextOverflow.Ellipsis)
                    Text(if(host.online)"Online · ${host.agentCount} ${if(host.agentCount==1)"agent" else "agents"} available" else "Offline",style=MaterialTheme.typography.bodySmall,color=if(host.online)ConnectedGreen else MaterialTheme.colorScheme.onSurfaceVariant,maxLines=2,overflow=TextOverflow.Ellipsis)
                }
                Icon(Icons.Outlined.ChevronRight,"Open connection",Modifier.size(22.dp),tint=MaterialTheme.colorScheme.onSurfaceVariant)
            } }
            item {TextButton(onAdd,Modifier.padding(start=12.dp,top=12.dp)) {Icon(Icons.Outlined.Add,null,Modifier.size(18.dp));Spacer(Modifier.width(6.dp));Text("Add connection",style=MaterialTheme.typography.titleMedium)}}
            if(hosts.isEmpty())item {Text("Pair your computer to start a coding session.",Modifier.padding(horizontal=24.dp,vertical=16.dp),style=MaterialTheme.typography.bodyMedium,color=MaterialTheme.colorScheme.onSurfaceVariant)}
        }
    }
}
@Composable fun PairScreen(loading:Boolean,onBack:()->Unit,onPair:(String,String,String)->Unit,initialAddress:String="",initialLabel:String="") {
    var address by rememberSaveable(initialAddress) { mutableStateOf(initialAddress) };var code by remember { mutableStateOf("") };var label by rememberSaveable(initialLabel) { mutableStateOf(initialLabel) }
    Column(Modifier.fillMaxSize().imePadding()) {
        ScreenHeader(if(initialAddress.isBlank())"Add connection" else "Pair connection again",onBack=onBack)
        LazyColumn(Modifier.fillMaxWidth().padding(horizontal=24.dp),verticalArrangement=Arrangement.spacedBy(20.dp)) {
            item { Text("Start acpd, then run acpd pair on that machine. Enter its HTTPS address and pairing code below.",style=MaterialTheme.typography.bodyMedium,color=MaterialTheme.colorScheme.onSurfaceVariant) }
            item { OutlinedTextField(label={Text("Connection name")},value=label,onValueChange={label=it},singleLine=true,enabled=!loading,modifier=Modifier.fillMaxWidth(),placeholder={Text("My development machine")}) }
            item { OutlinedTextField(label={Text("Address")},value=address,onValueChange={address=it},singleLine=true,enabled=!loading,modifier=Modifier.fillMaxWidth(),placeholder={Text("https://dev.example.com:8765")}) }
            item { OutlinedTextField(label={Text("Pairing code")},value=code,onValueChange={code=it.uppercase()},singleLine=true,enabled=!loading,modifier=Modifier.fillMaxWidth(),placeholder={Text("XXXX-XXXX-XXXX")}) }
            item { Button(onClick={onPair(address,code,label)},enabled=!loading && address.isNotBlank() && code.isNotBlank(),modifier=Modifier.fillMaxWidth().heightIn(min=52.dp)) { Text(if(loading)"Connecting…" else "Pair connection") } }
        }
    }
}
@Composable fun HostDetailsScreen(details:HostDetails?,onBack:()->Unit,onNew:()->Unit,onSession:(SessionInfo)->Unit,onRefresh:()->Unit,onForget:()->Unit,onAgent:((String)->Unit)?=null) {
    var confirmForget by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxSize()) {
        ScreenHeader(details?.host?.label ?: "Host",details?.host?.address,onBack,action={IconButton(onRefresh) { Icon(Icons.Outlined.Refresh,"Refresh host") }})
        if(details==null) EmptyState("Connecting to host","Agent discovery and workspace access are loading.")
        else LazyColumn {
            item { FilledTonalButton(onNew,Modifier.padding(horizontal=24.dp,vertical=8.dp)) { Icon(Icons.Outlined.Add,null);Spacer(Modifier.width(8.dp));Text("New session") } }
            item { SectionLabel("Workspaces") }
            items(details.workspaces) { workspace -> ListItem(headlineContent={Text(workspace.name.ifBlank { workspace.path })},supportingContent={Text(workspace.path,fontFamily=FontFamily.Monospace,maxLines=2)},leadingContent={Icon(Icons.Outlined.Folder,null)}) }
            item { SectionLabel("Agents") }
            items(details.agents) { agent -> ListItem(modifier=if(onAgent!=null)Modifier.clickable {onAgent(agent.id)} else Modifier,headlineContent={Text(agent.name)},supportingContent={Text(if(!agent.enabled)"Disabled" else agent.status.replace('_',' '))},leadingContent={Icon(Icons.Outlined.Code,null)},trailingContent={if(onAgent!=null)Icon(Icons.Outlined.ChevronRight,"Inspect ${agent.name}")}) }
            item { SectionLabel("Active sessions") }
            if(details.sessions.isEmpty()) item { Text("No sessions yet.",Modifier.padding(horizontal=24.dp,vertical=12.dp),color=MaterialTheme.colorScheme.onSurfaceVariant) }
            items(details.sessions,key={it.id}) { session -> ListItem(modifier=Modifier.clickable { onSession(session) },headlineContent={Text(session.workspace.substringAfterLast('/').substringAfterLast('\\'))},supportingContent={Text("${session.agentId} · ${session.statusLabel()}")},trailingContent={Icon(Icons.Outlined.ChevronRight,"Open session")}) }
            item { TextButton(onClick={confirmForget=true},modifier=Modifier.padding(16.dp)) { Text("Remove paired host",color=MaterialTheme.colorScheme.error) } }
        }
    }
    if(confirmForget) AlertDialog(onDismissRequest={confirmForget=false},title={Text("Remove this host?")},text={Text("This revokes this phone's credential and removes its local cache. Running host sessions continue.")},confirmButton={TextButton({confirmForget=false;onForget()}) { Text("Remove") }},dismissButton={TextButton({confirmForget=false}) { Text("Cancel") }})
}
@Composable fun AgentPicker(agents:List<AgentInfo>,onSelect:(AgentInfo)->Unit) {
    LazyColumn {
        item { SectionLabel("Choose agent") }
        items(agents,key={it.id}) { agent -> ListItem(modifier=Modifier.clickable(enabled=agent.canStartSession()) { onSelect(agent) },headlineContent={Text(agent.name)},supportingContent={Text(registryStatus(agent))},leadingContent={Icon(Icons.Outlined.Code,null)}) }
        item { Text("Add custom ACP agents to the host registry. They appear here after refresh.",Modifier.padding(24.dp),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant) }
    }
}
@Composable fun WorkspacePicker(workspaces:List<Workspace>,currentPath:String?,onSelect:(String)->Unit,onBrowse:(String)->Unit,loading:Boolean=false,error:String?=null,onRetry:(()->Unit)?=null) {
    LazyColumn(Modifier.testTag("workspace-browser-folders")) {
        item { SectionLabel("Choose workspace") }
        currentPath?.let { path ->
            item {Text(path,Modifier.padding(horizontal=24.dp),style=MaterialTheme.typography.bodySmall,fontFamily=FontFamily.Monospace,maxLines=3,overflow=TextOverflow.Ellipsis)}
            item { TextButton({onSelect(path)},Modifier.padding(horizontal=16.dp),enabled=!loading && error==null) { Text("Use this folder") } }
        }
        when {
            loading -> item {Row(Modifier.padding(24.dp),verticalAlignment=Alignment.CenterVertically,horizontalArrangement=Arrangement.spacedBy(12.dp)) {CircularProgressIndicator(Modifier.size(20.dp),strokeWidth=2.dp);Text("Loading folders",style=MaterialTheme.typography.bodyMedium)}}
            error!=null -> item {Column(Modifier.padding(horizontal=24.dp,vertical=12.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {Text("Could not open this folder",style=MaterialTheme.typography.titleMedium);Text(error,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant);onRetry?.let {TextButton(it) {Text("Retry")}}}}
            else -> {
                items(workspaces,key={it.path}) { workspace -> ListItem(modifier=Modifier.clickable { onSelect(workspace.path) },headlineContent={Text(workspace.name.ifBlank { workspace.path })},supportingContent={Text(workspace.path,maxLines=2,fontFamily=FontFamily.Monospace)},leadingContent={Icon(Icons.Outlined.Folder,null)},trailingContent={IconButton({onBrowse(workspace.path)}) { Icon(Icons.Outlined.ChevronRight,"Browse ${workspace.name}") }}) }
                if(workspaces.isEmpty()) item { Text(if(currentPath==null)"No allowed folders are available. Configure workspace roots on the host." else "No subfolders. You can use the current folder.",Modifier.padding(24.dp)) }
            }
        }
    }
}
@Composable fun NewSessionScreen(details:HostDetails?,browsed:List<Workspace>?,loading:Boolean,onBack:()->Unit,onBrowse:(HostProfile,String)->Unit,onCreate:(String,String)->Unit,onMcp:(()->Unit)?=null,onAccess:((String)->Unit)?=null,initialAgentId:String?=null,browseError:String?=null,browseLoading:Boolean=loading,onDismissBrowse:()->Unit={}) {
    var agentId by rememberSaveable {mutableStateOf(initialAgentId.orEmpty())}
    var path by rememberSaveable {mutableStateOf("")}
    var workspacePicker by remember {mutableStateOf(false)}
    var browsePath by rememberSaveable {mutableStateOf<String?>(null)}
    LaunchedEffect(details?.host?.id) {
        if(agentId.isBlank())agentId=details?.agents?.firstOrNull {it.canStartSession()}?.id.orEmpty()
        if(path.isBlank())path=details?.workspaces?.firstOrNull()?.path.orEmpty()
    }
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val scrollActions=maxHeight<420.dp || LocalDensity.current.fontScale>1.3f
        Column(Modifier.fillMaxSize()) {
            ScreenHeader("New session",onBack=onBack)
            if(details==null)EmptyState("Loading connection","Waiting for agents and workspaces.") else {
                val recents=recentWorkspaceChoices(details,initialAgentId).take(5)
                val startAction:@Composable ()->Unit={
                    Button({onCreate(agentId,path)},enabled=!loading && details.agents.any {it.id==agentId && it.canStartSession()} && workspaceInCurrentRoots(details.host.id,path,details.workspaces),shape=androidx.compose.foundation.shape.RoundedCornerShape(10.dp),modifier=Modifier.fillMaxWidth().padding(horizontal=24.dp,vertical=if(scrollActions)12.dp else 24.dp).heightIn(min=52.dp)) {Text("Start session")}
                }
                val actions:@Composable ()->Unit={
                    Row {
                        onAccess?.let {callback->TextButton({callback(path)},Modifier.padding(start=12.dp),enabled=!loading && path.isNotBlank()) {Text("Workspace access")}}
                        onMcp?.let {TextButton(it,enabled=!loading) {Text("MCP servers")}}
                    }
                    startAction()
                }
                LazyColumn(Modifier.weight(1f).testTag("new-session-choices"),contentPadding=PaddingValues(bottom=24.dp)) {
                    if(recents.isNotEmpty()) {
                        item {SectionLabel("Recent")}
                        items(recents,key={"recent:${it.agentId}:${it.path}"}) {recent->
                            val agent=details.agents.first {it.id==recent.agentId}
                            Row(Modifier.fillMaxWidth().testTag("recent:${recent.agentId}:${recent.path}").clickable {path=recent.path;agentId=recent.agentId}.padding(horizontal=24.dp,vertical=12.dp).heightIn(min=48.dp),verticalAlignment=Alignment.CenterVertically) {
                                Icon(Icons.Outlined.History,null,Modifier.size(22.dp));Spacer(Modifier.width(16.dp))
                                Column(Modifier.weight(1f),verticalArrangement=Arrangement.spacedBy(4.dp)) {
                                    Text(recent.path.trimEnd('/','\\').substringAfterLast('/').substringAfterLast('\\'),style=MaterialTheme.typography.titleMedium,maxLines=1,overflow=TextOverflow.Ellipsis)
                                    Text(agent.name,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                                    Text(recent.path,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant,maxLines=1,overflow=TextOverflow.Ellipsis)
                                }
                                if(path==recent.path && agentId==recent.agentId)Icon(Icons.Outlined.Check,"Selected recent workspace",Modifier.size(20.dp))
                            }
                        }
                    }
                    item {SectionLabel("Workspace")}
                    items(details.workspaces,key={it.path}) {workspace->
                        Row(Modifier.fillMaxWidth().clickable {path=workspace.path}.padding(horizontal=24.dp,vertical=14.dp),verticalAlignment=Alignment.CenterVertically) {
                            Icon(Icons.Outlined.Folder,null,Modifier.size(22.dp))
                            Spacer(Modifier.width(16.dp))
                            Text(workspace.name.ifBlank {workspace.path},Modifier.weight(1f),style=MaterialTheme.typography.titleMedium)
                            if(path==workspace.path)Icon(Icons.Outlined.Check,"Selected workspace",Modifier.size(20.dp))
                            else Icon(Icons.Outlined.ChevronRight,null,Modifier.size(20.dp))
                        }
                    }
                    item {TextButton({workspacePicker=true;browsePath=null},Modifier.padding(start=12.dp)) {Text("Browse folders",style=MaterialTheme.typography.bodySmall)}}
                    if(path.isNotBlank() && details.workspaces.none {it.path==path})item {Text(path,Modifier.padding(horizontal=24.dp),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}
                    item {SectionLabel("Agent")}
                    items(details.agents,key={it.id}) {agent->
                        Row(Modifier.fillMaxWidth().testTag("agent-choice:${agent.id}").clickable(enabled=agent.canStartSession()) {agentId=agent.id}.padding(start=24.dp,end=12.dp,top=6.dp,bottom=6.dp),verticalAlignment=Alignment.CenterVertically) {
                            Column(Modifier.weight(1f)) {
                                Text(agent.name,style=MaterialTheme.typography.titleMedium,color=if(agent.canStartSession())MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant)
                                if(!agent.canStartSession())Text(registryStatus(agent),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                            RadioButton(agentId==agent.id,{agentId=agent.id},enabled=agent.canStartSession())
                        }
                    }
                    if(scrollActions) {
                        onAccess?.let {callback->item(key="workspace-access-action") {TextButton({callback(path)},Modifier.padding(start=12.dp),enabled=!loading && path.isNotBlank()) {Text("Workspace access")}}}
                        onMcp?.let {callback->item(key="mcp-action") {TextButton(callback,Modifier.padding(start=12.dp),enabled=!loading) {Text("MCP servers")}}}
                        item(key="start-action") {startAction()}
                    }
                }
                if(!scrollActions)actions()
                if(workspacePicker)AlertDialog(onDismissRequest={workspacePicker=false;onDismissBrowse()},confirmButton={},dismissButton={TextButton({workspacePicker=false;onDismissBrowse()}) {Text("Close")}},text={Box(Modifier.heightIn(max=500.dp)) {WorkspacePicker(if(browsePath==null)details.workspaces else browsed.orEmpty(),browsePath,onSelect={path=it;workspacePicker=false;onDismissBrowse()},onBrowse={browsePath=it;onBrowse(details.host,it)},loading=browsePath!=null && browseLoading,error=browseError.takeIf {browsePath!=null},onRetry={browsePath?.let {onBrowse(details.host,it)}})}})
            }
        }
    }
}
@OptIn(ExperimentalFoundationApi::class)
@Composable fun SessionsScreen(sessions:List<StoredSession>,hosts:List<HostProfile>,onOpen:(StoredSession)->Unit,onArchive:(StoredSession)->Unit,onResume:(StoredSession)->Unit,onDelete:(StoredSession)->Unit,onRefresh:()->Unit,onNew:(()->Unit)?=null,liveActivities:Map<String,LiveSessionActivity> = emptyMap(),initialUiState:StoredUiState=StoredUiState(),onFiltersChanged:((Boolean,Boolean,String)->Unit)?=null) {
    var now by remember {mutableLongStateOf(System.currentTimeMillis())}
    LaunchedEffect(Unit) {while(true) {delay(60_000);now=System.currentTimeMillis()}}
    var archived by rememberSaveable {mutableStateOf(initialUiState.sessionsArchived)}
    var activeOnly by rememberSaveable {mutableStateOf(initialUiState.sessionsActive)}
    var query by rememberSaveable {mutableStateOf(initialUiState.sessionsQuery.take(512))}
    LaunchedEffect(archived,activeOnly,query) {onFiltersChanged?.invoke(archived,activeOnly,query)}
    var delete by remember {mutableStateOf<StoredSession?>(null)}
    var screenMenu by remember {mutableStateOf(false)}
    val records=remember(sessions) {sessions.map {it to WireJson.decodeFromString<SessionInfo>(it.metadata)}}
    val keyboard=LocalSoftwareKeyboardController.current
    val focus=LocalFocusManager.current
    val visible=records.filter { (stored,info)->stored.archived==archived && (!activeOnly || (liveActivities["${stored.hostId}/${stored.id}"]!=LiveSessionActivity.STOPPED && info.status in listOf("ready","running","authentication_required"))) && (query.isBlank() || (info.workspace+" "+info.agentId+" "+stored.state).contains(query,ignoreCase=true)) }
    BoxWithConstraints(Modifier.fillMaxSize().imePadding()) {
    val scrollFilters=maxHeight<420.dp || LocalDensity.current.fontScale>1.3f
    val tabs:@Composable ()->Unit={
        Row(Modifier.padding(start=24.dp,top=16.dp,bottom=20.dp),horizontalArrangement=Arrangement.spacedBy(32.dp)) {
            listOf(false to "All sessions",true to "Active").forEach { (active,label)->Column(Modifier.clickable {activeOnly=active}.padding(vertical=10.dp)) {
                Text(label,style=MaterialTheme.typography.labelLarge,color=if(activeOnly==active)MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant)
                if(activeOnly==active)HorizontalDivider(Modifier.width(if(active)42.dp else 82.dp).padding(top=8.dp),color=MaterialTheme.colorScheme.onSurface)
            } }
        }
    }
    val filters:@Composable ()->Unit={Surface(color=MaterialTheme.colorScheme.surface) {Column {
        TextField(query,{query=it.take(512)},placeholder={Text("Search sessions",style=MaterialTheme.typography.bodyMedium)},leadingIcon={Icon(Icons.Outlined.Search,null,Modifier.size(20.dp))},trailingIcon={if(query.isNotEmpty())IconButton({query=""}) {Icon(Icons.Outlined.Close,"Clear session search",Modifier.size(20.dp))}},keyboardOptions=KeyboardOptions(imeAction=ImeAction.Search),keyboardActions=KeyboardActions(onSearch={keyboard?.hide();focus.clearFocus()}),singleLine=true,shape=androidx.compose.foundation.shape.RoundedCornerShape(8.dp),modifier=Modifier.fillMaxWidth().padding(horizontal=24.dp),colors=TextFieldDefaults.colors(focusedContainerColor=MaterialTheme.colorScheme.surfaceContainer,unfocusedContainerColor=MaterialTheme.colorScheme.surfaceContainer,focusedIndicatorColor=androidx.compose.ui.graphics.Color.Transparent,unfocusedIndicatorColor=androidx.compose.ui.graphics.Color.Transparent))
        if(!scrollFilters)tabs()
    }}}
    Column(Modifier.fillMaxSize()) {
        ScreenHeader(if(archived)"Archived sessions" else "Sessions",action={Row {
            onNew?.let {IconButton(it) {Icon(Icons.Outlined.Add,"New session",Modifier.size(20.dp))}}
            Box {IconButton({screenMenu=true}) {Icon(Icons.Outlined.MoreVert,"Session list actions",Modifier.size(20.dp))};DropdownMenu(screenMenu,{screenMenu=false}) {
                DropdownMenuItem(text={Text("Refresh sessions")},onClick={screenMenu=false;onRefresh()})
                DropdownMenuItem(text={Text(if(archived)"All sessions" else "Archived sessions")},onClick={screenMenu=false;archived=!archived})
            }}
        }})
        LazyColumn(Modifier.weight(1f).testTag("sessions-list")) {
        stickyHeader(key="session-filters") {filters()}
        if(scrollFilters)item(key="session-tabs") {tabs()}
        if(visible.isEmpty())item(key="empty-sessions") {
            val title=when {query.isNotBlank()->"No matching sessions";archived->"No archived sessions";activeOnly->"No active sessions";else->"No sessions yet"}
            val description=when {query.isNotBlank()->"Try another search or clear it to see your sessions.";archived->"Sessions you archive will appear here.";activeOnly->"Choose All sessions to view saved and interrupted conversations.";else->"Start a session from a paired connection."}
            Box(if(scrollFilters)Modifier.fillMaxWidth().heightIn(min=240.dp) else Modifier.fillParentMaxSize()) {EmptyState(title,description)}
        }
        else {
            visible.groupBy {it.first.hostId to it.second.workspace}.forEach { (group,entries)->
                item {Row(Modifier.fillMaxWidth().padding(horizontal=24.dp,vertical=12.dp),horizontalArrangement=Arrangement.SpaceBetween) {Text(group.second.substringAfterLast('/').substringAfterLast('\\'),style=MaterialTheme.typography.labelMedium,color=MaterialTheme.colorScheme.onSurfaceVariant);Text(hosts.find {it.id==group.first}?.label ?: "Connection",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}}
                items(entries,key={"${it.first.hostId}/${it.first.id}"}) { (stored,info)->
                    var menu by remember {mutableStateOf(false)}
                    val cached=remember(stored.state) {runCatching {WireJson.decodeFromString<SessionState>(stored.state)}.getOrDefault(SessionState())}
                    val title=cached.sessionInfo["title"].text().takeIf {it.isNotBlank()}?.lineSequence()?.firstOrNull()?.take(64) ?: cached.items.filterIsInstance<TimelineItem.Text>().firstOrNull {it.role=="user"}?.text?.lineSequence()?.firstOrNull()?.take(64) ?: "Agent session"
                    val activity=sessionActivityTime(stored,cached)
                    val liveActivity=liveActivities["${stored.hostId}/${stored.id}"]
                    val status=when {
                        liveActivity==LiveSessionActivity.READY && info.status=="authentication_required" -> info.statusLabel()
                        liveActivity!=null -> liveActivity.label
                        info.status=="running" -> "Last reported: Working"
                        else -> info.statusLabel()
                    }
                    Row(Modifier.fillMaxWidth().testTag("session-${info.id}").clickable {onOpen(stored)}.padding(start=24.dp,end=12.dp,top=10.dp,bottom=16.dp),verticalAlignment=Alignment.Top) {
                        Text("•",Modifier.padding(end=12.dp,top=2.dp),color=if(liveActivity in listOf(LiveSessionActivity.READY,LiveSessionActivity.WORKING,LiveSessionActivity.WAITING_APPROVAL))ConnectedGreen else MaterialTheme.colorScheme.onSurfaceVariant)
                        Column(Modifier.weight(1f),verticalArrangement=Arrangement.spacedBy(6.dp)) {Text(title,style=MaterialTheme.typography.titleMedium,maxLines=2,overflow=TextOverflow.Ellipsis);Text("${info.agentId} · $status",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant);activity?.let {Text("${if(it.reportedByAgent)"Agent updated" else "Saved"} ${activityTimeLabel(it.timestamp,now)}",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}}
                        Box {IconButton({menu=true},Modifier.size(48.dp)) {Icon(Icons.Outlined.MoreVert,"Session actions",Modifier.size(18.dp),tint=MaterialTheme.colorScheme.onSurfaceVariant)};DropdownMenu(menu,{menu=false}) {
                            DropdownMenuItem(text={Text(if(stored.archived)"Unarchive" else "Archive")},onClick={menu=false;onArchive(stored)})
                            if(info.supportsLoad() && info.acpSessionId.isNotBlank())DropdownMenuItem(text={Text("Resume in a new process")},onClick={menu=false;onResume(stored)})
                            DropdownMenuItem(text={Text("Terminate and delete",color=MaterialTheme.colorScheme.error)},onClick={menu=false;delete=stored})
                        }}
                    }
                }
            }
        }
        }
    }
    }
    delete?.let {stored->AlertDialog(onDismissRequest={delete=null},title={Text("Terminate this session?")},text={Text("The host agent process will stop and this phone's session cache will be removed.")},confirmButton={TextButton({delete=null;onDelete(stored)}) {Text("Terminate")}},dismissButton={TextButton({delete=null}) {Text("Cancel")}})}
}
@Composable fun SettingsScreen(settings:PortalSettings,onTheme:(String)->Unit,onCache:(Boolean)->Unit,onConnections:(()->Unit)?=null,onMcp:(()->Unit)?=null,onAccess:(()->Unit)?=null,onAgents:(()->Unit)?=null) {
    var appearance by remember {mutableStateOf(false)}
    var history by remember {mutableStateOf(false)}
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
        ScreenHeader("Settings")
        onConnections?.let {SettingsRow("Connections",onClick=it)}
        onAgents?.let {SettingsRow("Agents",onClick=it)}
        onMcp?.let {SettingsRow("MCP servers",onClick=it)}
        onAccess?.let {SettingsRow("Workspace access",onClick=it)}
        SettingsRow("Appearance",settings.theme.replaceFirstChar(Char::uppercase),onClick={appearance=true})
        SettingsRow("History on this device",onClick={history=true})
    }
    if(appearance)AlertDialog(onDismissRequest={appearance=false},title={Text("Appearance")},text={Column(Modifier.verticalScroll(rememberScrollState())) {listOf("system","light","dark").forEach {mode->Row(Modifier.fillMaxWidth().clickable {onTheme(mode);appearance=false}.padding(vertical=6.dp),verticalAlignment=Alignment.CenterVertically) {Text(mode.replaceFirstChar(Char::uppercase),Modifier.weight(1f));RadioButton(settings.theme==mode,{onTheme(mode);appearance=false})}}}},confirmButton={TextButton({appearance=false}) {Text("Done")}})
    if(history)AlertDialog(onDismissRequest={history=false},title={Text("History on this device")},text={Column(Modifier.verticalScroll(rememberScrollState()),verticalArrangement=Arrangement.spacedBy(16.dp)) {Text("Keep recent messages available when this phone is offline. Turning this off removes cached messages from the phone.",style=MaterialTheme.typography.bodyMedium);Row(verticalAlignment=Alignment.CenterVertically) {Text("Save messages",Modifier.weight(1f));PortalSwitch(settings.cacheMessages,onCache)}}},confirmButton={TextButton({history=false}) {Text("Done")}})
}
@Composable private fun SettingsRow(title:String,value:String?=null,onClick:()->Unit) {
    Row(Modifier.fillMaxWidth().clickable(onClick=onClick).padding(horizontal=24.dp,vertical=24.dp),verticalAlignment=Alignment.CenterVertically) {
        Text(title,Modifier.weight(1f),style=MaterialTheme.typography.titleMedium)
        value?.let {Text(it,Modifier.padding(end=12.dp),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}
        Icon(Icons.Outlined.ChevronRight,null,Modifier.size(20.dp),tint=MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
private val PreviewHost=HostProfile("host","Development machine","https://dev.example.com","alias","device",0,agentCount=2,online=true)
@Preview(showBackground=true) @Composable private fun HostsScreenPreview() { PortalTheme { HostsScreen(listOf(PreviewHost),{},{},{}) } }
@Preview(showBackground=true,uiMode=32) @Composable private fun HostDetailsPreview() { PortalTheme { HostDetailsScreen(HostDetails(PreviewHost,listOf(AgentInfo("goose","Goose",installed=true,status="available")),listOf(Workspace("/projects/app","app")),emptyList(),emptyList()),{},{},{},{},{}) } }
@Preview(showBackground=true) @Composable private fun AgentPickerPreview() { PortalTheme { AgentPicker(listOf(AgentInfo("goose","Goose",installed=true,status="available"),AgentInfo("future","Custom ACP Agent")),{}) } }
@Preview(showBackground=true) @Composable private fun WorkspacePickerPreview() { PortalTheme { WorkspacePicker(listOf(Workspace("/projects/app","app")),null,{},{}) } }
@Preview(showBackground=true) @Composable private fun SessionsScreenPreview() { PortalTheme { SessionsScreen(emptyList(),emptyList(),{},{},{},{},{}) } }
@Preview(showBackground=true) @Composable private fun ConnectionErrorPreview() { PortalTheme { ConnectionError("The host is offline. Check its address and try again.",{}) } }
@Preview(showBackground=true) @Composable private fun EmptyStatePreview() { PortalTheme { EmptyState("No hosts yet","Pair your development machine to get started.") } }

