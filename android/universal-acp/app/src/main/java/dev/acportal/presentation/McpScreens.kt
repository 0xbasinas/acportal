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
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import dev.acportal.data.*
import dev.acportal.storage.HostProfile
import java.util.UUID

@Composable fun ConnectionsPickerScreen(hosts:List<HostProfile>,onBack:()->Unit,onHost:(String)->Unit,title:String="MCP servers") {
    Column(Modifier.fillMaxSize()) {
        ScreenHeader(title,onBack=onBack)
        Column(Modifier.verticalScroll(rememberScrollState()).padding(24.dp),verticalArrangement=Arrangement.spacedBy(16.dp)) {
            Text("Choose a connection",color=MaterialTheme.colorScheme.onSurfaceVariant,style=MaterialTheme.typography.bodySmall)
            if(hosts.isEmpty())Text("Add a connection to get started.")
            hosts.forEach {host->Row(Modifier.fillMaxWidth().clickable {onHost(host.id)}.padding(vertical=16.dp),verticalAlignment=Alignment.CenterVertically) {
                Text(host.label,Modifier.weight(1f),style=MaterialTheme.typography.bodyLarge)
                Icon(Icons.Outlined.ChevronRight,null,Modifier.size(20.dp))
            }}
        }
    }
}

@Composable fun McpServersScreen(hostLabel:String,servers:List<McpDefinition>,loading:Boolean,onBack:()->Unit,onSave:(List<McpDefinition>,()->Unit)->Unit) {
    var selected by remember(hostLabel) {mutableStateOf<McpDefinition?>(null)}
    val editing=selected
    androidx.activity.compose.BackHandler(editing!=null) {selected=null}
    if(editing!=null) {
        McpEditorScreen(editing,loading,{selected=null},{definition->onSave(servers.filter {it.id!=definition.id}+definition,{selected=null})},if(servers.any {it.id==editing.id}){{onSave(servers.filter {it.id!=editing.id},{selected=null})}} else null)
        return
    }
    Column(Modifier.fillMaxSize()) {
        ScreenHeader("MCP servers",hostLabel,onBack)
        Column(Modifier.verticalScroll(rememberScrollState()).padding(24.dp),verticalArrangement=Arrangement.spacedBy(24.dp)) {
            Text("For new or resumed sessions",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            if(servers.isEmpty())Text("No MCP servers added",style=MaterialTheme.typography.titleMedium)
            servers.forEach {server->Row(Modifier.fillMaxWidth().clickable(enabled=!loading) {selected=server}.padding(vertical=12.dp),verticalAlignment=Alignment.CenterVertically) {
                Column(Modifier.weight(1f),verticalArrangement=Arrangement.spacedBy(8.dp)) {
                    Text(server.name,style=MaterialTheme.typography.bodyLarge)
                    Text(if(server.enabled)server.displayEndpoint() else "Disabled",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                }
                Column(horizontalAlignment=Alignment.End,verticalArrangement=Arrangement.spacedBy(8.dp)) {
                    Text(server.transport.uppercase(),style=MaterialTheme.typography.labelSmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                    Icon(Icons.Outlined.ChevronRight,null,Modifier.size(20.dp))
                }
            }}
            TextButton({selected=McpDefinition(UUID.randomUUID().toString(),"",endpoint="")},enabled=!loading && servers.size<16,contentPadding=PaddingValues(0.dp)) {Text("+  Add server")}
            Text("HTTP and SSE depend on agent support. The host checks support when starting a session.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            Text("Existing sessions keep their server setup. Definitions are encrypted on this device.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

@Composable private fun McpEditorScreen(server:McpDefinition,loading:Boolean,onBack:()->Unit,onSave:(McpDefinition)->Unit,onDelete:(()->Unit)?) {
    var name by rememberSaveable(server.id) {mutableStateOf(server.name)}
    var transport by rememberSaveable(server.id) {mutableStateOf(server.transport)}
    var endpoint by remember(server.id) {mutableStateOf(server.endpoint)}
    var arguments by remember(server.id) {mutableStateOf(server.args.joinToString("\n"))}
    // Secret values are deliberately excluded from saved instance state.
    var values by remember(server.id) {mutableStateOf(server.values.joinToString("\n") {"${it.name}=${it.value}"})}
    var enabled by rememberSaveable(server.id) {mutableStateOf(server.enabled)}
    var error by remember(server.id) {mutableStateOf<String?>(null)}
    val formScroll=rememberScrollState()
    LaunchedEffect(error) {if(error!=null) {withFrameNanos {};formScroll.animateScrollTo(formScroll.maxValue)}}
    BoxWithConstraints(Modifier.fillMaxSize().imePadding()) {
        val scrollActions=maxHeight<420.dp || LocalDensity.current.fontScale>1.3f
        val saveAction:@Composable ()->Unit={
            Button({try {
                val definition=McpDefinition(server.id,name.trim(),transport,endpoint.trim(),if(transport=="stdio" && arguments.isNotEmpty())arguments.split('\n') else emptyList(),parseMcpValues(values),enabled)
                definition.wire();error=null;onSave(definition)
            } catch(failure:IllegalArgumentException) {error=failure.message ?: "Check the server definition"}},Modifier.fillMaxWidth().padding(horizontal=if(scrollActions)0.dp else 24.dp,vertical=if(scrollActions)8.dp else 24.dp).heightIn(min=52.dp),enabled=!loading,shape=RoundedCornerShape(10.dp)) {Text("Save server")}
        }
        Column(Modifier.fillMaxSize()) {
        ScreenHeader(if(onDelete==null)"Add server" else "Edit server",onBack=onBack)
        Column(Modifier.weight(1f).verticalScroll(formScroll).padding(24.dp),verticalArrangement=Arrangement.spacedBy(16.dp)) {
            OutlinedTextField(name,{name=it},label={Text("Server name")},singleLine=true,modifier=Modifier.fillMaxWidth(),enabled=!loading)
            Text("Transport",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            FlowRow(horizontalArrangement=Arrangement.spacedBy(4.dp)) {listOf("stdio","http","sse").forEach {kind->TextButton({if(transport!=kind) {transport=kind;endpoint="";values=""}},enabled=!loading) {Text(if(transport==kind)"✓ ${kind.uppercase()}" else kind.uppercase())}}}
            OutlinedTextField(endpoint,{endpoint=it},label={Text(if(transport=="stdio")"Executable on host" else "Server URL")},singleLine=true,modifier=Modifier.fillMaxWidth(),enabled=!loading)
            if(transport=="stdio") {
                OutlinedTextField(arguments,{arguments=it},label={Text("Arguments")},supportingText={Text("One argument per line. Passed literally to the executable.")},minLines=2,maxLines=6,modifier=Modifier.fillMaxWidth(),enabled=!loading)
            }
            OutlinedTextField(values,{values=it},label={Text(if(transport=="stdio")"Environment variables" else "Headers")},supportingText={Text("One NAME=value per line")},minLines=2,maxLines=6,modifier=Modifier.fillMaxWidth(),enabled=!loading)
            Row(Modifier.fillMaxWidth(),verticalAlignment=Alignment.CenterVertically) {Text("Use for new sessions",Modifier.weight(1f));PortalSwitch(enabled,{enabled=it},enabled=!loading)}
            error?.let {Text(it,color=MaterialTheme.colorScheme.error,style=MaterialTheme.typography.bodySmall)}
            onDelete?.let {TextButton(it,enabled=!loading) {Text("Remove server")}}
            if(scrollActions)saveAction()
        }
        if(!scrollActions)saveAction()
        }
    }
}
