package dev.acportal.presentation

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ChevronRight
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.acportal.storage.HostProfile

@Composable internal fun SelectionCard(title:String,subtitle:String,onClick:()->Unit) {
    Surface(shape=RoundedCornerShape(12.dp),color=MaterialTheme.colorScheme.surfaceContainer,modifier=Modifier.fillMaxWidth().padding(horizontal=24.dp).clickable(onClick=onClick)) {
        Row(Modifier.padding(16.dp).heightIn(min=48.dp),verticalAlignment=Alignment.CenterVertically) {
            Column(Modifier.weight(1f),verticalArrangement=Arrangement.spacedBy(4.dp)) {
                Text(title,style=MaterialTheme.typography.titleMedium)
                if(subtitle.isNotBlank())Text(subtitle,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Icon(Icons.Outlined.ChevronRight,null)
        }
    }
}

@Composable fun SessionConnectionScreen(hosts:List<HostProfile>,onBack:()->Unit,onPair:()->Unit,onChoose:(String)->Unit) {
    Column(Modifier.fillMaxSize()) {
        ScreenHeader("New session","Choose a connection",onBack)
        LazyColumn(Modifier.weight(1f),contentPadding=PaddingValues(24.dp),verticalArrangement=Arrangement.spacedBy(16.dp)) {
            if(hosts.isEmpty()) {
                item {Text("Pair your first connection",style=MaterialTheme.typography.titleLarge)}
                item {Text("Connect this phone to your host to choose a workspace and agent.",color=MaterialTheme.colorScheme.onSurfaceVariant)}
            } else {
                item {Text("Where should this session run?")}
                items(hosts,key={it.id}) {host->
                    Surface(shape=RoundedCornerShape(12.dp),color=MaterialTheme.colorScheme.surfaceContainer,modifier=Modifier.fillMaxWidth().clickable {onChoose(host.id)}) {
                        Row(Modifier.padding(16.dp).heightIn(min=64.dp),verticalAlignment=Alignment.CenterVertically) {
                            Column(Modifier.weight(1f),verticalArrangement=Arrangement.spacedBy(8.dp)) {
                                Text(host.label,style=MaterialTheme.typography.titleMedium)
                                Text(if(host.online)"Online · available to connect" else "Offline · check connection",style=MaterialTheme.typography.bodySmall,color=if(host.online)ConnectedGreen else MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                            Icon(Icons.Outlined.ChevronRight,null)
                        }
                    }
                }
                item {Text("Connection details are refreshed before a session can start.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}
            }
            item {OutlinedButton(onPair,Modifier.fillMaxWidth().heightIn(min=52.dp)) {Text(if(hosts.isEmpty())"Pair a connection" else "Pair another connection")}}
        }
    }
}
