package dev.acportal.presentation

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.Send
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.ui.Modifier
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.acportal.data.LiveSession
import dev.acportal.data.saveTranscript
import dev.acportal.protocol.*
import dev.acportal.storage.StoredSession
import dev.acportal.transport.ConnectionState
import kotlinx.serialization.json.*
import kotlinx.coroutines.*

@Composable fun SessionScreen(live:LiveSession,stored:StoredSession,onBack:()->Unit,onPrompt:(String,String?)->Unit,onCancel:()->Unit,onPermission:(Permission,String)->Unit,onConfigure:(String,JsonObject)->Unit,onDraft:(String)->Unit,onReconnect:()->Unit={},onDisconnect:()->Unit={},onMcp:(()->Unit)?=null,onAccess:(()->Unit)?=null,onRemoveLocal:(()->Unit)?=null,onAddAttachment:((PromptAttachment,Long)->Unit)?=null,onRemoveAttachment:((Int)->Unit)?=null,onDismissError:()->Unit={},onSessions:()->Unit=onBack,onPairHost:(()->Unit)?=null,onElicitation:(Permission,String,JsonObject?)->Unit={_,_,_->}) {
    val diagnostics by live.diagnostics.collectAsStateWithLifecycle()
    val state by live.state.collectAsStateWithLifecycle()
    val connection by live.connection.collectAsStateWithLifecycle()
    val info by live.metadata.collectAsStateWithLifecycle()
    val authenticating by live.authenticating.collectAsStateWithLifecycle()
    val generation by live.localCopyGeneration.collectAsStateWithLifecycle()
    val attachments by live.attachments.collectAsStateWithLifecycle()
    TranscriptExportSurface(info,state) {exporting,onExport->
        SessionContent(info,state,connection,stored.draft,onBack,onPrompt,onCancel,onPermission,onConfigure,onDraft,authenticating,live.host.label,live.host.address,diagnostics,onReconnect,onDisconnect,onMcp,onAccess,onRemoveLocal,generation,exporting,onExport=onExport,attachments=attachments,onAddAttachment=onAddAttachment,onRemoveAttachment=onRemoveAttachment,onDismissError=onDismissError,onSessions=onSessions,onPairHost=onPairHost,onElicitation=onElicitation)
    }
}

@Composable private fun TranscriptExportSurface(info:SessionInfo,state:SessionState,content:@Composable (Boolean,()->Unit)->Unit) {
    val context=LocalContext.current
    val scope=rememberCoroutineScope()
    val snackbar=remember {SnackbarHostState()}
    var snapshot by remember {mutableStateOf<String?>(null)}
    var exporting by remember {mutableStateOf(false)}
    val destination=rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("text/markdown")) {uri->
        val text=snapshot;snapshot=null
        if(uri==null)exporting=false
        else scope.launch {
            val message=try {check(text!=null);saveTranscript(context,uri,text);"Transcript saved"}
                catch(cancelled:CancellationException) {throw cancelled}
                catch(_:Exception) {"Transcript could not be saved. Try exporting again."}
            finally {exporting=false}
            snackbar.showSnackbar(message)
        }
    }
    Box(Modifier.fillMaxSize()) {
        content(exporting) {
            val capturedInfo=info;val capturedState=state
            exporting=true
            scope.launch {
                try {snapshot=withContext(Dispatchers.Default) {transcriptMarkdown(capturedInfo,capturedState)};destination.launch("acportal-${capturedInfo.id.take(8)}.md")}
                catch(cancelled:CancellationException) {throw cancelled}
                catch(_:Exception) {snapshot=null;exporting=false;snackbar.showSnackbar("Transcript could not be prepared. It may exceed the export size limit.")}
            }
        }
        SnackbarHost(snackbar,Modifier.align(Alignment.BottomCenter).padding(16.dp))
    }
}

@Composable fun SavedSessionScreen(info:SessionInfo,stored:StoredSession,host:dev.acportal.storage.HostProfile?,loading:Boolean,onBack:()->Unit,onDraft:(String)->Unit,onRetry:()->Unit,onRetryLabel:String="Connect to host") {
    val cached=remember(stored.state) {runCatching {WireJson.decodeFromString<SessionState>(stored.state)}.getOrDefault(SessionState())}
    val display=cached.copy(processing=false,replaying=false,permissions=emptyMap(),modelRequests=emptyMap(),error=null)
    TranscriptExportSurface(info,cached) {exporting,onExport->
        SessionContent(info,display,ConnectionState.Disconnected,stored.draft,onBack,{_,_->},{},{_,_->},{_,_->},onDraft,
            hostLabel=host?.label,hostAddress=host?.address.orEmpty(),onReconnect=onRetry,offlineCopy=true,checkingConnection=loading,exporting=exporting,onExport=onExport,savedActionLabel=onRetryLabel)
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable private fun SessionContent(info:SessionInfo,state:SessionState,connection:ConnectionState,initialDraft:String,onBack:()->Unit,onPrompt:(String,String?)->Unit,onCancel:()->Unit,onPermission:(Permission,String)->Unit,onConfigure:(String,JsonObject)->Unit,onDraft:(String)->Unit,authenticating:Boolean=false,hostLabel:String?=null,hostAddress:String="",diagnostics:List<dev.acportal.transport.ConnectionDiagnostic> = emptyList(),onReconnect:()->Unit={},onDisconnect:()->Unit={},onMcp:(()->Unit)?=null,onAccess:(()->Unit)?=null,onRemoveLocal:(()->Unit)?=null,draftResetVersion:Long=0,exporting:Boolean=false,onExport:(()->Unit)?=null,attachments:List<PromptAttachment> = emptyList(),onAddAttachment:((PromptAttachment,Long)->Unit)?=null,onRemoveAttachment:((Int)->Unit)?=null,onDismissError:()->Unit={},onSessions:()->Unit=onBack,onPairHost:(()->Unit)?=null,offlineCopy:Boolean=false,checkingConnection:Boolean=false,savedActionLabel:String="Connect to host",onElicitation:(Permission,String,JsonObject?)->Unit={_,_,_->}) {
    var draft by rememberSaveable(info.id) { mutableStateOf(initialDraft) }
    val focus=LocalFocusManager.current
    val keyboard=LocalSoftwareKeyboardController.current
    val keyboardVisible=WindowInsets.ime.getBottom(LocalDensity.current)>0
    var attachmentLoading by remember(info.id) {mutableStateOf(false)}
    var sentDraft by remember(info.id) {mutableStateOf<Pair<Long,String>?>(null)}
    val list=rememberLazyListState()
    val conversationUi=rememberSaveableStateHolder()
    var followLatest by rememberSaveable(info.id) {mutableStateOf(true)}
    var dragging by remember(info.id) {mutableStateOf(false)}
    var showPermission by remember(info.id) {mutableStateOf(false)}
    var showMenu by remember(info.id) {mutableStateOf(false)}
    var showHistory by remember(info.id) {mutableStateOf(false)}
    var historyChoice by remember(info.id) {mutableStateOf<String?>(null)}
    var showOptions by remember(info.id) {mutableStateOf(false)}
    var showAgent by remember(info.id) {mutableStateOf(false)}
    var connectionPage by remember(info.id) {mutableIntStateOf(0)}
    var removeCopy by remember(info.id) {mutableStateOf(false)}
    var changedFile by rememberSaveable(info.id) {mutableStateOf<String?>(null)}
    var handledReset by rememberSaveable(info.id) {mutableLongStateOf(draftResetVersion)}
    LaunchedEffect(draftResetVersion) {if(draftResetVersion>handledReset) {draft="";sentDraft=null;showHistory=false;historyChoice=null;focus.clearFocus();keyboard?.hide();handledReset=draftResetVersion}}
    val pendingPermission=state.permissions.values.firstOrNull()
    LaunchedEffect(pendingPermission?.id) {showPermission=pendingPermission!=null}
    val connected=connection is ConnectionState.Connected && !state.sessionClosed && info.status!="exited"
    LaunchedEffect(state.items) { val sent=sentDraft;val accepted=state.items.filterIsInstance<TimelineItem.Text>().lastOrNull {it.role=="user"};if(sent!=null && state.sequence>sent.first && accepted?.text==sent.second) {draft="";onDraft("");sentDraft=null} }
    val status=if(offlineCopy)"Saved conversation" else if(info.status=="exited")"Stopped" else dev.acportal.data.liveSessionActivity(state,connection).label
    LaunchedEffect(list) { list.interactionSource.interactions.collect { interaction->when(interaction) {
        is DragInteraction.Start->{dragging=true;followLatest=false}
        is DragInteraction.Stop,is DragInteraction.Cancel->{dragging=false;followLatest=!list.canScrollForward}
    } } }
    LaunchedEffect(list) {snapshotFlow {list.layoutInfo.visibleItemsInfo.isNotEmpty() && !list.canScrollForward && !dragging}.collect {atEnd->if(atEnd)followLatest=true}}
    LaunchedEffect(state.items.size,state.items.lastOrNull(),followLatest) {if(followLatest && state.items.isNotEmpty())list.scrollToItem(state.items.lastIndex)}
    androidx.activity.compose.BackHandler(changedFile!=null || showOptions || showAgent || connectionPage>0) {if(changedFile!=null)changedFile=null else if(connectionPage>0)connectionPage-- else {showOptions=false;showAgent=false}}
    changedFile?.let {id->ChangesScreen(sessionFileChanges(state),id,{changedFile=null});return}
    if(connectionPage==2) {ConnectionLogsScreen(diagnostics,{connectionPage=1},hostLabel ?: "Host",connection);return}
    if(connectionPage==1) {ConnectionDetailsScreen(hostLabel ?: "Host",hostAddress,connection,{connectionPage=0},{connectionPage=2},onReconnect,onDisconnect,info.initialization["agentInfo"].objectValue()["name"].text().ifBlank {info.agentId});return}
    if(showOptions) {SessionOptionsScreen(info,state,connected && !state.replaying && info.acpSessionId.isNotBlank(),{showOptions=false},onConfigure);return}
    if(showAgent) {AgentDetailsScreen(info,{showAgent=false},hostLabel,state.usage,state.sessionInfo);return}
    conversationUi.SaveableStateProvider(info.id) {
    Column(Modifier.fillMaxSize().imePadding()) {
        ScreenHeader(state.sessionInfo["title"].text().takeIf {it.isNotBlank()}?.lineSequence()?.firstOrNull()?.take(64) ?: state.items.filterIsInstance<TimelineItem.Text>().firstOrNull {it.role=="user"}?.text?.lineSequence()?.firstOrNull()?.take(36) ?: "New conversation","${info.workspace.trimEnd('/','\\').substringAfterLast('/').substringAfterLast('\\')} · ${info.agentId}",onBack,action={Box {
            IconButton({showMenu=true}) {Icon(Icons.Outlined.MoreVert,"Session menu",Modifier.size(20.dp))}
            DropdownMenu(showMenu,{showMenu=false},modifier=Modifier.widthIn(min=224.dp,max=280.dp),shape=RoundedCornerShape(16.dp),containerColor=MaterialTheme.colorScheme.surfaceContainer) {
                DropdownMenuItem(text={Text("Session options")},leadingIcon={Icon(Icons.Outlined.Tune,null,Modifier.size(18.dp))},onClick={showMenu=false;showOptions=true})
                onAccess?.let {callback->DropdownMenuItem(text={Text("Workspace access")},leadingIcon={Icon(Icons.Outlined.Shield,null,Modifier.size(18.dp))},onClick={showMenu=false;callback()})}
                DropdownMenuItem(text={Text("Connection details")},leadingIcon={Icon(Icons.Outlined.Computer,null,Modifier.size(18.dp))},onClick={showMenu=false;connectionPage=1})
                DropdownMenuItem(text={Text("Agent details")},leadingIcon={Icon(Icons.Outlined.Info,null,Modifier.size(18.dp))},onClick={showMenu=false;showAgent=true})
                onMcp?.let {callback->DropdownMenuItem(text={Text("MCP servers")},leadingIcon={Icon(Icons.Outlined.Extension,null,Modifier.size(18.dp))},onClick={showMenu=false;callback()})}
                onExport?.let {callback->DropdownMenuItem(text={Text(if(exporting)"Preparing transcript" else "Export transcript")},enabled=!exporting,leadingIcon={Icon(Icons.Outlined.FileDownload,null,Modifier.size(18.dp))},onClick={showMenu=false;callback()})}
                onRemoveLocal?.let {HorizontalDivider(Modifier.padding(horizontal=12.dp),color=MaterialTheme.colorScheme.outlineVariant);DropdownMenuItem(text={Text("Remove local copy",color=MaterialTheme.colorScheme.error)},leadingIcon={Icon(Icons.Outlined.DeleteOutline,null,Modifier.size(18.dp),tint=MaterialTheme.colorScheme.error)},onClick={showMenu=false;removeCopy=true})}
            }
        }})
        HorizontalDivider(color=MaterialTheme.colorScheme.outlineVariant)
        Row(Modifier.padding(horizontal=20.dp).fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween) {
            Text(status,style=MaterialTheme.typography.labelMedium,color=MaterialTheme.colorScheme.onSurfaceVariant)
            if(state.replaying)Text("Restoring conversation",style=MaterialTheme.typography.labelMedium)
        }
        if(offlineCopy && !keyboardVisible)Surface(Modifier.fillMaxWidth().padding(horizontal=16.dp,vertical=8.dp),shape=RoundedCornerShape(12.dp),color=MaterialTheme.colorScheme.surfaceContainer) {
            Column(Modifier.padding(16.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {
                Text(if(info.status=="interrupted")"This conversation was saved before the host restarted. Open Session actions to resume or start again." else "Showing messages saved on this device. Connect to check the latest activity.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                TextButton(onReconnect,enabled=!checkingConnection) {Text(if(checkingConnection)"Checking connection…" else savedActionLabel)}
            }
        } else if(!offlineCopy)SessionRecovery(state,connection,onDismissError,onReconnect,onSessions,onPairHost)
        if(info.acpSessionId.isBlank() && !offlineCopy) {
            AuthenticationPanel(info,connected && !state.replaying,authenticating) { method->onConfigure("authenticate",buildJsonObject { put("methodId",method) }) }
        }

        if(info.ownTools && !offlineCopy)OwnToolsNotice(Modifier.padding(horizontal=16.dp,vertical=4.dp),compact=state.items.isNotEmpty())
        if(state.localHistoryCleared)Text("Earlier messages were removed from this device.",Modifier.padding(horizontal=20.dp,vertical=8.dp),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
        if(state.historyGap)Text("Some earlier messages are unavailable in this conversation.",Modifier.padding(16.dp),style=MaterialTheme.typography.bodySmall)
        if(state.items.isEmpty())Box(Modifier.weight(1f).fillMaxWidth()) { if(offlineCopy)EmptyState("No messages saved","Connect to view this conversation. Message caching can be changed in Settings.") else if(info.acpSessionId.isNotBlank())EmptyState("Ready to work","Ask the agent to explore or change your workspace.") }
        else LazyColumn(state=list,modifier=Modifier.weight(1f).fillMaxWidth().testTag("session-timeline"),contentPadding=PaddingValues(16.dp),verticalArrangement=Arrangement.spacedBy(12.dp)) {
            items(state.items,key={it.id}) { item -> when(item) {
                is TimelineItem.Text->if(item.role=="thought")AgentThoughtCard(item.id) {androidx.compose.foundation.text.selection.SelectionContainer {Text(item.text,style=MaterialTheme.typography.bodyMedium)}} else MessageCard(item,info.agentId)
                is TimelineItem.Content->if(item.role=="thought")AgentThoughtCard(item.id) {ReceivedContentView(item.content)} else Column(Modifier.padding(8.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {Text(if(item.role=="user")"You" else info.agentId,style=MaterialTheme.typography.labelSmall,color=MaterialTheme.colorScheme.onSurfaceVariant);ReceivedContentView(item.content)}
                is TimelineItem.Tool->ToolCard(item,state.terminals,onChange={changedFile=it})
                is TimelineItem.Plan->OutlinedCard { Column(Modifier.padding(16.dp)) { Text("Plan",style=MaterialTheme.typography.titleSmall);item.entries.forEach { entry->val value=entry.objectValue();Text("${if(value["status"].text()=="completed") "✓" else "•"} ${value["content"].text()}",Modifier.padding(top=8.dp)) } } }
                is TimelineItem.Notice->Text(item.text,color=if(item.error)MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant)
            } }
        }
        if(pendingPermission!=null && !showPermission)TextButton({showPermission=true},Modifier.align(Alignment.CenterHorizontally)) {Text(if(pendingPermission.isElicitation)"Answer the agent's question" else "Review permission")}
        Surface(Modifier.fillMaxWidth().padding(horizontal=16.dp,vertical=12.dp),shape=RoundedCornerShape(20.dp),color=MaterialTheme.colorScheme.surfaceContainer,border=BorderStroke(1.dp,MaterialTheme.colorScheme.outlineVariant)) {Column(Modifier.padding(8.dp)) {
            onRemoveAttachment?.let {AttachmentRows(attachments,!state.processing && !state.replaying,it)}
            TextField(draft,{draft=it;onDraft(it)},placeholder={Text("Message, @files",style=MaterialTheme.typography.bodyMedium)},modifier=Modifier.fillMaxWidth(),minLines=1,maxLines=6,enabled=!state.replaying,colors=TextFieldDefaults.colors(focusedContainerColor=Color.Transparent,unfocusedContainerColor=Color.Transparent,disabledContainerColor=Color.Transparent,focusedIndicatorColor=Color.Transparent,unfocusedIndicatorColor=Color.Transparent,disabledIndicatorColor=Color.Transparent))
            Row(Modifier.fillMaxWidth(),verticalAlignment=Alignment.CenterVertically) {
                onAddAttachment?.let {AttachmentControls(info,connected && info.acpSessionId.isNotBlank() && !state.processing && !state.replaying,draftResetVersion,{attachmentLoading=it},it,onHistory=if(!state.replaying && !state.sessionClosed) {{showHistory=true}} else null)}
                Text(info.agentId,Modifier.weight(1f),style=MaterialTheme.typography.labelMedium,color=MaterialTheme.colorScheme.onSurfaceVariant)
                if(state.processing)IconButton(onCancel,enabled=connected) { Icon(Icons.Outlined.Stop,"Stop turn") }
                else IconButton({sentDraft=state.sequence to promptDisplay(promptContent(draft,attachments));focus.clearFocus();keyboard?.hide();onPrompt(draft,null)},enabled=connected && info.acpSessionId.isNotBlank() && (draft.isNotBlank() || attachments.isNotEmpty()) && !state.replaying && !attachmentLoading) { Icon(Icons.Outlined.ArrowUpward,"Send message",Modifier.size(22.dp)) }
            }
        } }
    }
    }
    if(showHistory)PromptHistorySheet(promptHistory(state),{showHistory=false}) {text->
        showHistory=false
        if(draft.isNotBlank() && draft!=text)historyChoice=text
        else {draft=text;sentDraft=null;onDraft(text)}
    }
    historyChoice?.let {text->AlertDialog(onDismissRequest={historyChoice=null},title={Text("Replace draft?")},text={Text("Replace the current text with this earlier prompt. Selected attachments stay in the composer.")},confirmButton={TextButton({draft=text;sentDraft=null;onDraft(text);historyChoice=null}) {Text("Replace")}},dismissButton={TextButton({historyChoice=null}) {Text("Cancel")}})}
    if(removeCopy)AlertDialog(onDismissRequest={removeCopy=false},title={Text("Remove local copy?")},text={Text("Remove this phone's messages and draft. The host keeps the session and its history. Pending permission decisions remain available.")},confirmButton={TextButton({removeCopy=false;onRemoveLocal?.invoke()}) {Text("Remove",color=MaterialTheme.colorScheme.error)}},dismissButton={TextButton({removeCopy=false}) {Text("Cancel")}})
    if(showPermission && pendingPermission!=null)ModalBottomSheet(onDismissRequest={showPermission=false},sheetState=rememberModalBottomSheetState(skipPartiallyExpanded=true),containerColor=MaterialTheme.colorScheme.surfaceContainerLow) {
        Column(Modifier.verticalScroll(rememberScrollState()).imePadding()) {
            if(pendingPermission.isElicitation)ElicitationRequest(pendingPermission,connected && !state.replaying) {action,content->focus.clearFocus();keyboard?.hide();onElicitation(pendingPermission,action,content)}
            else PermissionRequest(pendingPermission,connected && !state.replaying) {focus.clearFocus();keyboard?.hide();onPermission(pendingPermission,it)}
            Spacer(Modifier.navigationBarsPadding().height(16.dp))
        }
    }
}

@Composable fun AuthenticationPanel(info:SessionInfo,enabled:Boolean,authenticating:Boolean,onSelect:(String)->Unit) {
    Column(Modifier.fillMaxWidth().heightIn(max=240.dp).verticalScroll(rememberScrollState()).padding(16.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {
        Text("Sign in to ${info.agentId}",style=MaterialTheme.typography.titleMedium)
        Text(if(authenticating)"Complete the agent's sign-in flow on the host. This page will update when it finishes." else "This agent needs authentication before the session can start.",style=MaterialTheme.typography.bodySmall)
        if(authenticating)LinearProgressIndicator(Modifier.fillMaxWidth())
        val methods=info.authenticationMethods()
        if(methods.isEmpty())Text("Complete sign-in on the host, then start a new session.",style=MaterialTheme.typography.bodySmall)
        methods.forEach { method ->
            method["description"].text().takeIf { it.isNotBlank() }?.let { Text(it,style=MaterialTheme.typography.bodySmall) }
            OutlinedButton({onSelect(method["id"].text())},enabled=enabled && !authenticating) { Text(method["name"].text().ifBlank { "Sign in" }) }
        }
    }
}

@Composable fun InterruptedSessionScreen(info:SessionInfo,loading:Boolean,onBack:()->Unit,onResume:()->Unit,onCreate:()->Unit,onViewSaved:(()->Unit)?=null) {
    Column(Modifier.fillMaxSize()) {
        ScreenHeader("Session interrupted",info.agentId,onBack)
        Column(Modifier.padding(24.dp).verticalScroll(rememberScrollState()),verticalArrangement=Arrangement.spacedBy(16.dp)) {
            Text("The host restarted and this agent process stopped.",style=MaterialTheme.typography.titleMedium)
            Text(info.workspace,style=MaterialTheme.typography.bodyMedium)
            onViewSaved?.let {TextButton(it) {Text("View saved conversation")}}
            if(info.supportsLoad() && info.acpSessionId.isNotBlank()) {
                Text("Resume the agent's saved session in a new process. Pending commands and permission choices will not be repeated.")
                Button(onResume,enabled=!loading) {Text("Resume session")}
                OutlinedButton(onCreate,enabled=!loading) {Text("Start new session")}
            } else {
                Text("This agent has no saved session available to load. Start a new session in this workspace.")
                Button(onCreate,enabled=!loading) {Text("Start new session")}
            }
        }
    }
}

@Composable internal fun AgentThoughtCard(id:String,content:@Composable ()->Unit) {
    var expanded by rememberSaveable(id) {mutableStateOf(false)}
    Surface(Modifier.fillMaxWidth(),shape=RoundedCornerShape(12.dp),color=MaterialTheme.colorScheme.surfaceContainer) {
        Column {
            TextButton({expanded=!expanded},Modifier.fillMaxWidth().heightIn(min=48.dp).semantics {stateDescription=if(expanded)"Expanded" else "Collapsed"}) {
                Text("Agent thoughts",Modifier.weight(1f),style=MaterialTheme.typography.labelLarge,color=MaterialTheme.colorScheme.onSurfaceVariant)
                Icon(if(expanded)Icons.Outlined.ExpandLess else Icons.Outlined.ExpandMore,if(expanded)"Hide agent thoughts" else "Show agent thoughts")
            }
            if(expanded)Column(Modifier.padding(start=16.dp,end=16.dp,bottom=16.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {content()}
        }
    }
}

@Composable private fun MessageCard(item:TimelineItem.Text,agent:String) {
    val user=item.role=="user"
    Box(Modifier.fillMaxWidth(),contentAlignment=if(user)Alignment.CenterEnd else Alignment.CenterStart) {
        Surface(color=if(user)MaterialTheme.colorScheme.surfaceContainer else Color.Transparent,shape=RoundedCornerShape(8.dp),modifier=if(user)Modifier.fillMaxWidth(0.88f) else Modifier.fillMaxWidth()) {
            Column(Modifier.padding(if(user)16.dp else 8.dp)) {Text(if(user)"You" else agent,style=MaterialTheme.typography.labelSmall,color=MaterialTheme.colorScheme.onSurfaceVariant);androidx.compose.foundation.text.selection.SelectionContainer {Text(item.text,Modifier.padding(top=8.dp),style=MaterialTheme.typography.bodyLarge)}}
        }
    }
}

@Composable fun ToolCard(tool:TimelineItem.Tool,terminals:Map<String,JsonObject> = emptyMap(),onChange:((String)->Unit)?=null) {
    var expanded by rememberSaveable(tool.id) { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth().testTag("tool-card")) {
        ListItem(headlineContent={Text(tool.title)},supportingContent={Column {Text("${tool.kind} · ${tool.status}");tool.hostWrite?.let {value->hostWriteLabel(value)?.let {label->Text(label,Modifier.testTag("host-write-badge"),style=MaterialTheme.typography.labelMedium,color=if(value==HOST_WRITE_CHANGED)MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant)}};tool.autoReview?.let {Text(autoReviewLabel(it),Modifier.testTag("auto-review-badge"),style=MaterialTheme.typography.labelMedium,color=if(it["decision"].text()=="deny")MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary)}}},leadingContent={Icon(Icons.Outlined.Build,null)},trailingContent={IconButton({expanded=!expanded},Modifier.sizeIn(minWidth=48.dp,minHeight=48.dp).testTag("tool-disclosure-${tool.id}").semantics {stateDescription=if(expanded)"Expanded" else "Collapsed"}) {Icon(if(expanded)Icons.Outlined.ExpandLess else Icons.Outlined.ExpandMore,(if(expanded)"Collapse tool: " else "Expand tool: ")+tool.title)}})
        // A write the user refused that is already on disk stays visible without expanding.
        if(tool.hostWrite==HOST_WRITE_CHANGED)hostWriteExplanation(HOST_WRITE_CHANGED)?.let {text->
            Surface(Modifier.fillMaxWidth().padding(horizontal=16.dp).testTag("host-write-notice"),shape=RoundedCornerShape(8.dp),color=MaterialTheme.colorScheme.errorContainer,contentColor=MaterialTheme.colorScheme.onErrorContainer) {
                Row(Modifier.padding(12.dp),horizontalArrangement=Arrangement.spacedBy(12.dp)) {Icon(Icons.Outlined.WarningAmber,null,Modifier.size(20.dp));Text(text,style=MaterialTheme.typography.bodySmall)}
            }
        }
        if(expanded)Column(Modifier.padding(16.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {
            if(tool.hostWrite==HOST_WRITE_UNCHANGED)hostWriteExplanation(HOST_WRITE_UNCHANGED)?.let {Text(it,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}
            tool.locations.forEach { Text(it.objectValue()["path"].text(),style=MaterialTheme.typography.labelSmall) }
            tool.autoReview?.let {review->
                if(review["reason"].text().isNotBlank())Text("Reason: "+review["reason"].text(),style=MaterialTheme.typography.bodySmall)
                if(review["shellLine"].text().isNotEmpty())TerminalOutput(review["shellLine"].text())
            }
            tool.content.forEachIndexed {index,content->val block=content.objectValue();when(block["type"].text()) {
                "diff"->{DiffViewer(block["path"].text(),block["oldText"]?.takeUnless {it==JsonNull}?.text(),block["newText"].text());onChange?.let {callback->TextButton({callback("${tool.id}/$index")}) {Icon(Icons.Outlined.OpenInFull,null,Modifier.size(18.dp));Spacer(Modifier.width(8.dp));Text("View changes")}}}
                "content"->ReceivedContentView(block["content"].objectValue())
                "terminal"->TerminalSnapshot(terminals[block["terminalId"].text()],tool.status in listOf("completed","failed"))
                else->Text("Additional tool content",style=MaterialTheme.typography.bodySmall)
            } }
            if(tool.content.isEmpty())Text("No output yet",style=MaterialTheme.typography.bodySmall)
        }
    }
}

@Composable fun TerminalSnapshot(snapshot:JsonObject?,completed:Boolean=false) {
    if(snapshot==null) {Text(if(completed)"Terminal output is no longer cached" else "Waiting for terminal output",style=MaterialTheme.typography.bodySmall);return}
    Column(verticalArrangement=Arrangement.spacedBy(8.dp)) {
        if(snapshot["truncated"].flag() || snapshot["output"].text().length>24000)Text("Earlier output omitted",style=MaterialTheme.typography.labelSmall)
        TerminalOutput(snapshot["output"].text())
        snapshot["exitStatus"]?.objectValue()?.let { status->
            val code=status["exitCode"].takeUnless {it==null || it==JsonNull}?.text()
            val signal=status["signal"].takeUnless {it==null || it==JsonNull}?.text()
            Text(code?.let {"Exit code $it"} ?: signal?.let {"Stopped by signal $it"} ?: "Command stopped",style=MaterialTheme.typography.labelSmall)
        }
    }
}

@Composable fun PermissionRequest(permission:Permission,enabled:Boolean=true,onSelect:(String)->Unit) {
    var details by remember(permission.id) {mutableStateOf(false)}
    var review by remember(permission.id) {mutableStateOf(false)}
    var fullReview by remember(permission.id) {mutableStateOf(false)}
    val proposedChanges=remember(permission) {permissionFileChanges(permission)}
    if(fullReview && proposedChanges.isNotEmpty())Dialog(onDismissRequest={fullReview=false},properties=DialogProperties(usePlatformDefaultWidth=false)) {
        Surface(Modifier.fillMaxSize()) {ChangesScreen(proposedChanges,proposedChanges.first().id,{fullReview=false})}
    }
    Column(Modifier.fillMaxWidth().padding(horizontal=24.dp,vertical=16.dp),verticalArrangement=Arrangement.spacedBy(12.dp)) {
            Text(if(permission.request["_meta"].objectValue()["acpdSource"].text().isNotBlank())"Host permission needed" else "Permission needed",Modifier.semantics {heading()},style=MaterialTheme.typography.labelLarge)
            Text(permission.title,Modifier.padding(vertical=8.dp),style=MaterialTheme.typography.titleLarge)
            permission.request["toolCall"].objectValue()["locations"].arrayValue().forEach { Text(it.objectValue()["path"].text(),style=MaterialTheme.typography.bodySmall) }
            val rawInput=permission.request["toolCall"].objectValue()["rawInput"]
            val operation=rawInput.objectValue()
            val shellApproval=remember(permission) {shellCommandApproval(permission)}
            if(shellApproval!=null)ShellCommandReview(shellApproval)
            else if(operation["command"].text().isNotBlank())Text((operation["command"].text()+" "+operation["args"].arrayValue().joinToString(" ") {it.text()}).take(2000),Modifier.padding(vertical=16.dp),style=MaterialTheme.typography.bodyMedium)
            if(rawInput!=null && rawInput!=JsonNull) {TextButton({details=!details}) {Text(if(details)"Hide operation details" else "Operation details")};if(details)TerminalOutput(rawInput.toString())}
            val diffs=permission.request["toolCall"].objectValue()["content"].arrayValue().filter {it.objectValue()["type"].text()=="diff"}
            if(diffs.isNotEmpty()) {TextButton({review=!review}) {Text(if(review)"Hide proposed change" else "Review proposed change")};if(review)Column(Modifier.heightIn(max=240.dp).verticalScroll(rememberScrollState())) {diffs.forEach {entry->val diff=entry.objectValue();DiffViewer(diff["path"].text(),diff["oldText"]?.takeUnless {it==JsonNull}?.text(),diff["newText"].text())}}}
            if(review && proposedChanges.isNotEmpty())TextButton({fullReview=true}) {Text("Open full change")}
            Spacer(Modifier.height(12.dp))
            permission.options.sortedBy {if(it.objectValue()["kind"].text().startsWith("allow"))0 else 1}.forEach {option->val value=option.objectValue();val name=value["name"].text().ifBlank {value["kind"].text().replace('_',' ')}
                if(value["kind"].text().startsWith("allow"))Button({onSelect(value["optionId"].text())},enabled=enabled,modifier=Modifier.fillMaxWidth().heightIn(min=52.dp),shape=RoundedCornerShape(10.dp)) {Text(name)}
                else TextButton({onSelect(value["optionId"].text())},enabled=enabled,modifier=Modifier.fillMaxWidth().heightIn(min=48.dp)) {Text(name)}
            }
    }
}

/** The full shell line, never shortened, scrollable and selectable. */
@Composable fun ShellCommandReview(approval:ShellCommandApproval) {
    Column(Modifier.fillMaxWidth().testTag("shell-command-review"),verticalArrangement=Arrangement.spacedBy(8.dp)) {
        Surface(color=MaterialTheme.colorScheme.errorContainer,shape=RoundedCornerShape(6.dp)) {Text("Shell command",Modifier.padding(horizontal=8.dp,vertical=4.dp),color=MaterialTheme.colorScheme.onErrorContainer,style=MaterialTheme.typography.labelMedium)}
        Text(approval.runsWith(),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
        Surface(color=MaterialTheme.colorScheme.surfaceContainerHigh,contentColor=MaterialTheme.colorScheme.onSurface,shape=RoundedCornerShape(8.dp),modifier=Modifier.fillMaxWidth()) {
            androidx.compose.foundation.text.selection.SelectionContainer {
                Text(approval.line,Modifier.testTag("shell-command-line").heightIn(max=320.dp).verticalScroll(rememberScrollState()).padding(12.dp),color=MaterialTheme.colorScheme.onSurface,fontFamily=FontFamily.Monospace,style=MaterialTheme.typography.bodyMedium,softWrap=true)
            }
        }
        approval.autoReviewReason?.let {Text("Not auto-approved — $it",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}
        if(approval.environmentNames.isNotEmpty())Text("Environment: "+approval.environmentNames.joinToString(", "),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
        Text("Approve only if you want this exact line to run once on your host.",style=MaterialTheme.typography.bodySmall)
    }
}

@Composable fun DiffViewer(path:String,oldText:String?,newText:String) {
    val lines=remember(oldText,newText) { unifiedDiff(oldText,newText) }
    val addedColor=if(MaterialTheme.colorScheme.surface.luminance()<0.5f)ConnectedGreen else Color(0xFF28613C)
    Column { Text(path,style=MaterialTheme.typography.labelLarge);Column(Modifier.horizontalScroll(rememberScrollState()).padding(top=8.dp)) {
        lines.take(1000).forEach { line->Text("${line.oldLine?.toString()?.padStart(4) ?: "    "} ${line.newLine?.toString()?.padStart(4) ?: "    "} ${when(line.kind) {DiffKind.ADDED->"+";DiffKind.REMOVED->"−";else->" "}} ${line.text}",fontFamily=FontFamily.Monospace,style=MaterialTheme.typography.bodySmall,color=when(line.kind) {DiffKind.ADDED->addedColor;DiffKind.REMOVED->MaterialTheme.colorScheme.error;else->MaterialTheme.colorScheme.onSurface}) }
        if(lines.size>1000)Text("Preview shows the first 1,000 lines.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
        if(lines.size>1000)Text("Preview limited to 1,000 lines")
    } }
}
@Composable fun TerminalOutput(text:String) { androidx.compose.foundation.text.selection.SelectionContainer { Text(text.take(64000),Modifier.horizontalScroll(rememberScrollState()),fontFamily=FontFamily.Monospace,style=MaterialTheme.typography.bodySmall) } }

@Preview(showBackground=true) @Composable private fun SessionPreview() { PortalTheme("Light") {SessionContent(SessionInfo("1","goose","s1","/workspace/project"),SessionState(items=listOf(TimelineItem.Text("1","user","Explain this project"),TimelineItem.Text("2","agent","I’ll inspect the project structure and entry points."))),ConnectionState.Connected,"",{},{_,_->},{},{_,_->},{_,_->},{})} }
@Preview(showBackground=true) @Composable private fun PermissionPreview() { PortalTheme("Dark") {PermissionRequest(Permission(JsonPrimitive(1),WireJson.parseToJsonElement("""{"toolCall":{"title":"Run project tests"},"options":[{"optionId":"allow","name":"Allow once","kind":"allow_once"},{"optionId":"deny","name":"Reject","kind":"reject_once"}]}""").objectValue())) {}} }
@Preview(showBackground=true) @Composable private fun DiffPreview() { PortalTheme("Light") {DiffViewer("src/main.kt","fun greet() = \"Hi\"","fun greet() = \"Hello\"")} }
@Preview(showBackground=true) @Composable private fun TerminalPreview() { PortalTheme("Dark") {TerminalOutput("BUILD SUCCESSFUL\n27 tests passed")} }
@Preview(showBackground=true) @Composable private fun AuthenticationPreview() { PortalTheme("Dark") {AuthenticationPanel(SessionInfo("auth","agent","","workspace",initialization=WireJson.parseToJsonElement("""{"authMethods":[{"id":"login","name":"Sign in","description":"Continue with the agent's sign-in flow on your development host."}]}""").objectValue()),true,false,{})} }




