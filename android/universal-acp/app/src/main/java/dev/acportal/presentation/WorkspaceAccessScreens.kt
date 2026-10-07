package dev.acportal.presentation

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ChevronRight
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.unit.dp
import dev.acportal.data.HostDetails
import dev.acportal.protocol.*

@Composable fun WorkspaceAccessListScreen(details:HostDetails?,browsed:List<Workspace>?,loading:Boolean,onBack:()->Unit,onBrowse:(String)->Unit,onOpen:(String)->Unit,browseError:String?=null,browseLoading:Boolean=loading,onDismissBrowse:()->Unit={},onReload:()->Unit={}) {
    var browse by remember {mutableStateOf(false)}
    var path by remember {mutableStateOf<String?>(null)}
    Column(Modifier.fillMaxSize()) {
        ScreenHeader("Workspace access",details?.host?.label,onBack)
        if(details==null) {
            if(loading)EmptyState("Loading workspaces","Reading the connection's allowed folders.")
            else {
                Box(Modifier.weight(1f)) {EmptyState("Workspaces unavailable","Reload this connection's folders or return to choose another connection.")}
                TextButton(onReload,Modifier.fillMaxWidth().padding(24.dp)) {Text("Reload workspaces")}
            }
        }
        else Column(Modifier.verticalScroll(rememberScrollState()).padding(24.dp),verticalArrangement=Arrangement.spacedBy(16.dp)) {
            Text("Choose a workspace",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            if(details.workspaces.isEmpty())Text("No allowed workspaces",style=MaterialTheme.typography.titleMedium)
            details.workspaces.forEach {workspace->Row(Modifier.fillMaxWidth().clickable(enabled=!loading) {onOpen(workspace.path)}.padding(vertical=16.dp),verticalAlignment=Alignment.CenterVertically) {
                Text(workspace.name,Modifier.weight(1f),style=MaterialTheme.typography.bodyLarge)
                Icon(Icons.Outlined.ChevronRight,null,Modifier.size(20.dp))
            }}
            TextButton({path=null;browse=true},enabled=!loading,contentPadding=PaddingValues(0.dp)) {Text("Browse folders")}
        }
    }
    if(browse && details!=null)AlertDialog(onDismissRequest={browse=false;onDismissBrowse()},confirmButton={},dismissButton={TextButton({browse=false;onDismissBrowse()}) {Text("Close")}},text={Box(Modifier.heightIn(max=500.dp)) {
        WorkspacePicker(if(path==null)details.workspaces else browsed.orEmpty(),path,{browse=false;onDismissBrowse();onOpen(it)},{path=it;onBrowse(it)},loading=path!=null && browseLoading,error=browseError.takeIf {path!=null},onRetry={path?.let(onBrowse)})
    }})
}

@Composable fun WorkspaceAccessScreen(hostLabel:String,path:String,saved:WorkspaceAccess,current:WorkspaceAccess?,loading:Boolean,onBack:()->Unit,onSave:(WorkspaceAccess)->Unit) {
    var draft by remember(path,saved) {mutableStateOf(saved)}
    BoxWithConstraints(Modifier.fillMaxSize()) {
    val scrollActions=maxHeight<420.dp || LocalDensity.current.fontScale>1.3f
    val saveAction:@Composable ()->Unit={
        Button({onSave(draft)},Modifier.fillMaxWidth().padding(horizontal=if(scrollActions)0.dp else 24.dp,vertical=if(scrollActions)8.dp else 24.dp).heightIn(min=52.dp),shape=RoundedCornerShape(10.dp),enabled=!loading) {Text(if(current==null)"Save access" else "Save for next session")}
    }
    Column(Modifier.fillMaxSize()) {
        ScreenHeader("Workspace access",onBack=onBack)
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(24.dp),verticalArrangement=Arrangement.spacedBy(16.dp)) {
            Text(path.trimEnd('/','\\').substringAfterLast('/').substringAfterLast('\\').ifEmpty {path},style=MaterialTheme.typography.titleMedium)
            Text(hostLabel,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.height(16.dp))
            AccessSwitch("Read files",draft.readFiles,!loading) {draft=draft.copy(readFiles=it)}
            AccessSwitch("Write files",draft.writeFiles,!loading) {draft=draft.copy(writeFiles=it)}
            AccessSwitch("Terminal",draft.terminal,!loading) {draft=draft.copy(terminal=it)}
            Spacer(Modifier.height(16.dp))
            Text("Tool approvals",style=MaterialTheme.typography.bodyLarge)
            Text("Ask each time",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.height(16.dp))
            Text("Applies to client-provided file and terminal access. These controls do not sandbox the agent process.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            Text("Saved changes apply to the next new or resumed session. Reconnecting this session keeps its current access.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            current?.let {Text("Current session: Read ${if(it.readFiles)"on" else "off"} · Write ${if(it.writeFiles)"on" else "off"} · Terminal ${if(it.terminal)"on" else "off"}",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}
            if(scrollActions)saveAction()
        }
        if(!scrollActions)saveAction()
    }
    }
}

@Composable private fun AccessSwitch(label:String,checked:Boolean,enabled:Boolean,onChange:(Boolean)->Unit) {
    Row(Modifier.fillMaxWidth().heightIn(min=48.dp),verticalAlignment=Alignment.CenterVertically) {
        Text(label,Modifier.weight(1f),style=MaterialTheme.typography.bodyLarge)
        PortalSwitch(checked,onChange,enabled=enabled,modifier=Modifier.semantics {contentDescription=label})
    }
}
