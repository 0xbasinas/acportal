package dev.acportal.presentation

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ChevronRight
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.semantics.Role
import dev.acportal.data.HostDetails
import dev.acportal.protocol.AgentInfo
import dev.acportal.protocol.canStartSession
import androidx.compose.foundation.text.selection.SelectionContainer

@Composable fun RegistryAgentsScreen(details:HostDetails,onBack:()->Unit,onRefresh:()->Unit,onAgent:(String)->Unit,savedAt:Long?=null) {
    BoxWithConstraints(Modifier.fillMaxSize()) {
    val scrollControls=maxHeight<420.dp || LocalDensity.current.fontScale>1.3f
    Column(Modifier.fillMaxSize()) {
        ScreenHeader("Agents",details.host.label,onBack,action={IconButton(onRefresh) {Icon(Icons.Outlined.Refresh,"Refresh agents")}})
        if(!scrollControls)savedAt?.let {SavedAgentNotice(it)}
        if(details.agents.isEmpty() && !scrollControls)EmptyState("No agents configured","Add an ACP agent to this computer's registry, then refresh.")
        else LazyColumn(Modifier.weight(1f).testTag("agent-list"),contentPadding=PaddingValues(bottom=24.dp)) {
            if(scrollControls)savedAt?.let {timestamp->item(key="saved-notice") {SavedAgentNotice(timestamp)}}
            if(details.agents.isEmpty()) {
                item(key="empty-title") {Text("No agents configured",Modifier.padding(horizontal=24.dp,vertical=16.dp),style=MaterialTheme.typography.titleLarge)}
                item(key="empty-help") {Text("Add an ACP agent to this computer's registry, then refresh.",Modifier.padding(horizontal=24.dp),style=MaterialTheme.typography.bodyMedium,color=MaterialTheme.colorScheme.onSurfaceVariant)}
            }
            items(details.agents,key={"agent:${it.id}"}) {agent->
                Row(Modifier.fillMaxWidth().clickable(role=Role.Button,onClickLabel="Inspect agent") {onAgent(agent.id)}.padding(horizontal=24.dp,vertical=20.dp),verticalAlignment=Alignment.CenterVertically) {
                    Column(Modifier.weight(1f).padding(end=12.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {
                        Text(agent.name,style=MaterialTheme.typography.titleMedium,maxLines=2,overflow=TextOverflow.Ellipsis)
                        Text(if(savedAt==null)registryStatus(agent) else "Last reported: ${registryStatus(agent)}",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant,maxLines=2,overflow=TextOverflow.Ellipsis)
                    }
                    Icon(Icons.Outlined.ChevronRight,null,Modifier.size(22.dp),tint=MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
    }
    }
}

@Composable fun RegistryAgentDetailsScreen(agent:AgentInfo,hostLabel:String,loading:Boolean,onBack:()->Unit,onRefresh:()->Unit,onNew:()->Unit,savedAt:Long?=null) {
    BoxWithConstraints(Modifier.fillMaxSize()) {
    val scrollControls=maxHeight<420.dp || LocalDensity.current.fontScale>1.3f
    val launch:@Composable ()->Unit={
        Button(onNew,enabled=!loading && savedAt==null && agent.canStartSession(),modifier=Modifier.fillMaxWidth().padding(if(scrollControls)0.dp else 24.dp).heightIn(min=52.dp)) {Text("New session")}
    }
    Column(Modifier.fillMaxSize()) {
        ScreenHeader("Agent details",onBack=onBack,action={IconButton(onRefresh,enabled=!loading) {Icon(Icons.Outlined.Refresh,"Refresh agent")}})
        if(!scrollControls)savedAt?.let {SavedAgentNotice(it)}
        Column(Modifier.weight(1f).testTag("agent-detail-body").verticalScroll(rememberScrollState()).padding(horizontal=24.dp,vertical=if(scrollControls)16.dp else 0.dp),verticalArrangement=Arrangement.spacedBy(16.dp)) {
            if(scrollControls)savedAt?.let {SavedAgentNotice(it,inBody=true)}
            Text(agent.name,style=MaterialTheme.typography.titleLarge)
            Text(hostLabel,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.height(16.dp))
            RegistryValue(if(savedAt==null)"Availability" else "Last reported availability",registryStatus(agent))
            RegistryValue("Agent ID",agent.id)
            RegistryValue("Transport","STDIO")
            agent.executable?.takeIf {it.isNotBlank()}?.let {value->
                Text("Executable",style=MaterialTheme.typography.bodyLarge)
                SelectionContainer {Text(value,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}
            }
            agent.version?.takeIf {it.isNotBlank()}?.let {RegistryValue("Version",it)}
            agent.reason?.takeIf {it.isNotBlank()}?.let {value->
                Text("Configuration problem",style=MaterialTheme.typography.titleMedium)
                SelectionContainer {Text(value,style=MaterialTheme.typography.bodyMedium,color=MaterialTheme.colorScheme.onSurfaceVariant)}
            }
            Spacer(Modifier.height(16.dp))
            Text("Capabilities",style=MaterialTheme.typography.titleMedium)
            Text("Start a session to see the capabilities, models and sign-in methods reported by this agent.",style=MaterialTheme.typography.bodyMedium,color=MaterialTheme.colorScheme.onSurfaceVariant)
            if(!agent.enabled)Text("Enable this agent in the computer's registry to start a session.",style=MaterialTheme.typography.bodyMedium)
            else if(agent.status=="misconfigured")Text("Correct this agent's configuration on the computer, then refresh.",style=MaterialTheme.typography.bodyMedium)
            else if(!agent.installed)Text("Install this CLI on the computer, or update its executable path in the registry, then refresh.",style=MaterialTheme.typography.bodyMedium)
            if(scrollControls)launch()
        }
        if(!scrollControls)launch()
    }
    }
}

@Composable private fun SavedAgentNotice(timestamp:Long,inBody:Boolean=false) {
    Text("Saved agent list · Last checked ${java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.SHORT,java.text.DateFormat.SHORT).format(java.util.Date(timestamp))}. Refresh to check this host.",Modifier.padding(horizontal=if(inBody)0.dp else 24.dp,vertical=12.dp),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
}

@Composable private fun RegistryValue(label:String,value:String) {
    Row(Modifier.fillMaxWidth().padding(vertical=10.dp),verticalAlignment=Alignment.CenterVertically) {
        Text(label,Modifier.weight(1f),style=MaterialTheme.typography.bodyLarge)
        Text(value,Modifier.weight(1f),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

internal fun registryStatus(agent:AgentInfo)=if(!agent.enabled)"Disabled" else if(agent.status=="misconfigured")"Configuration problem" else if(!agent.installed)"Not installed" else agent.status.replace('_',' ').replaceFirstChar(Char::uppercase)
