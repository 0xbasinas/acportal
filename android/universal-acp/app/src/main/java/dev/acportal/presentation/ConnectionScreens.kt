package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.clickable
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import dev.acportal.transport.*
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

@Composable fun ConnectionDetailsScreen(label:String,address:String,connection:ConnectionState,onBack:()->Unit,onLogs:()->Unit,onReconnect:()->Unit,onDisconnect:()->Unit,agent:String="") {
    Column(Modifier.fillMaxSize()) {
        ScreenHeader("Connection details",onBack=onBack)
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal=24.dp),verticalArrangement=Arrangement.spacedBy(24.dp)) {
            Text(label,style=MaterialTheme.typography.titleLarge)
            ConnectionStatus(connection)
            Spacer(Modifier.height(8.dp))
            ConnectionValue("Host",address)
            ConnectionValue("Connection","WebSocket")
            ConnectionValue("Agent",agent)
            ConnectionValue("Agent transport","STDIO")
            if(connection is ConnectionState.Failed)Text(connection.reason,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.error)
            TextButton(onLogs,contentPadding=PaddingValues(0.dp)) {Text("View connection logs")}
            Text("Disconnecting leaves the agent session running on the host.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Column(Modifier.padding(24.dp).fillMaxWidth(),verticalArrangement=Arrangement.spacedBy(8.dp)) {
            Button(onReconnect,Modifier.fillMaxWidth().height(52.dp),shape=RoundedCornerShape(10.dp),colors=ButtonDefaults.buttonColors(containerColor=MaterialTheme.colorScheme.surfaceContainer,contentColor=MaterialTheme.colorScheme.onSurface),enabled=connection !is ConnectionState.Connecting) {Text("Reconnect")}
            Button(onDisconnect,Modifier.fillMaxWidth().height(52.dp),shape=RoundedCornerShape(10.dp),colors=ButtonDefaults.buttonColors(containerColor=MaterialTheme.colorScheme.surfaceContainer,contentColor=MaterialTheme.colorScheme.onSurface),enabled=connection !is ConnectionState.Disconnected) {Text("Disconnect")}
        }
    }
}

@Composable private fun ConnectionValue(label:String,value:String) {
    Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween) {
        Text(label,style=MaterialTheme.typography.bodyLarge)
        Text(value,Modifier.weight(1f),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant,textAlign=androidx.compose.ui.text.style.TextAlign.End)
    }
}

@Suppress("DEPRECATION")
@Composable fun ConnectionLogsScreen(events:List<ConnectionDiagnostic>,onBack:()->Unit,label:String="",connection:ConnectionState=ConnectionState.Disconnected) {
    var filter by remember {mutableStateOf("All")}
    var live by remember {mutableStateOf(true)}
    var frozen by remember {mutableStateOf(events)}
    val shown=(if(live)events else frozen).filter {filter=="All" || it.level==if(filter=="Warnings")"Warning" else "Error"}
    val clipboard=LocalClipboardManager.current
    val format=remember {SimpleDateFormat("HH:mm:ss",Locale.getDefault())}
    Column(Modifier.fillMaxSize()) {
        ScreenHeader("Connection logs",onBack=onBack,action={TextButton({clipboard.setText(AnnotatedString(shown.joinToString("\n") {"${format.format(Date(it.time))} ${it.level}: ${it.message}"}))},enabled=shown.isNotEmpty()) {Text("Copy")}})
        Column(Modifier.padding(horizontal=24.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {Text(label,style=MaterialTheme.typography.titleMedium);ConnectionStatus(connection)}
        Row(Modifier.padding(horizontal=24.dp),horizontalArrangement=Arrangement.spacedBy(8.dp)) {
            listOf("All","Warnings","Errors").forEach {label->Column(Modifier.padding(top=24.dp,end=32.dp).selectable(selected=filter==label,role=Role.Tab,onClick={filter=label}).padding(vertical=12.dp)) {
                Text(label,color=if(filter==label)MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant)
                Spacer(Modifier.height(8.dp))
                if(filter==label)HorizontalDivider(Modifier.width(24.dp),color=MaterialTheme.colorScheme.onSurface)
            }}
        }
        LazyColumn(Modifier.weight(1f),contentPadding=PaddingValues(24.dp),verticalArrangement=Arrangement.spacedBy(20.dp)) {
            if(shown.isEmpty())item {Text("No matching events",color=MaterialTheme.colorScheme.onSurfaceVariant)}
            items(shown) {event->Column(verticalArrangement=Arrangement.spacedBy(6.dp)) {
                Text("${format.format(Date(event.time))} · ${event.level}",style=MaterialTheme.typography.labelSmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                Text(event.message,style=MaterialTheme.typography.bodyMedium)
            }}
        }
        Row(Modifier.fillMaxWidth().padding(horizontal=24.dp),horizontalArrangement=Arrangement.SpaceBetween) {
            Text("Live updates",Modifier.padding(top=12.dp),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            PortalSwitch(live,{enabled->if(!enabled)frozen=events;live=enabled})
        }
        Text("Connection events only · latest 200",Modifier.padding(24.dp),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable private fun ConnectionStatus(connection:ConnectionState) {
    Text(when(connection) {
        ConnectionState.Connected->"• Connected"
        ConnectionState.Connecting->"Connecting"
        ConnectionState.Disconnected->"Disconnected"
        is ConnectionState.Reconnecting->"Reconnecting"
        is ConnectionState.Failed->"Unavailable"
    },style=MaterialTheme.typography.bodySmall,color=if(connection is ConnectionState.Connected)Color(0xFF72D5B0) else MaterialTheme.colorScheme.onSurfaceVariant)
}
